"""ASTRA #043 regression preparation, run only by the reserved G115 worker.

Extract the two production readers verbatim. A std-only fixture executable also
acts as fake Git, with pipe capacities read from the actual Windows handles.
No native launch occurs in this module; the caller supplies its guarded run().
"""
import hashlib
from pathlib import Path
import struct

COUNT = 4096
PAYLOAD = 4096
CASES = [(site, outcome) for site in ('build', 'runtime')
         for outcome in ('success', 'writer-failure', 'child-failure')]
CONTRACT = b'git-ls-tree-r-z-path-mode-type-framed-blob-content-or-gitlink-oid-sha256/v1'


def production_functions(source):
    result = []
    for relative, name, end, typename in (
        ('mtg-kernel/build.rs', 'git_blob_contents', 'hash_frame', 'GitTreeEntry'),
        ('mtg-kernel/examples/bench_kernel/matched_uniform_runtime.rs',
         'runtime_git_blob_contents', 'tracked_tree_hash_frame', 'GitTreeEntryV3'),
    ):
        text = (Path(source) / relative).read_text(encoding='utf-8')
        begin = text.index('fn ' + name + '(')
        finish = text.index('\nfn ' + end + '(', begin)
        result.append(f'struct {typename} {{ mode: Vec<u8>, kind: Vec<u8>, '
                      'object_id: String, path: Vec<u8> }\n' + text[begin:finish])
    return '\n'.join(result)


def prepare(source, root):
    root = Path(root)
    root.mkdir()  # Fresh attempt only, no reuse of generated code or binaries.
    path = root / 'fixture.rs'
    path.write_text(RUST.replace('/* PRODUCTION_READERS */', production_functions(source)),
                    encoding='utf-8', newline='\n')
    for site, outcome in CASES:
        (root / (site + '-' + outcome)).mkdir()
    return path


def expected_frames():
    def frame(data):
        return struct.pack('>Q', len(data)) + data
    data = bytearray(CONTRACT + b'\0' + struct.pack('>Q', COUNT + 1))
    for i in range(COUNT):
        oid = f'{i:040x}'.encode()
        for value in (f'{i:05}'.encode(), b'100644', b'blob',
                      oid + bytes([i % 251]) * PAYLOAD):
            data.extend(frame(value))
    for value in (b'z-gitlink', b'160000', b'commit', b'f' * 40):
        data.extend(frame(value))
    return bytes(data)


def verify(root):
    expected = expected_frames()
    result = []
    for site, outcome in CASES:
        case = Path(root) / (site + '-' + outcome)
        assert (case / 'passed.txt').read_text() == site + ' ' + outcome
        row = {'site': site, 'outcome': outcome}
        if outcome == 'success':
            actual = (case / 'frames.bin').read_bytes()
            assert actual == expected, 'Ordered path/mode/type/blob/gitlink frames changed'
            capacities = [int(n) for n in (case / 'capacities.txt').read_text().split()]
            # Both directions are reported by GetNamedPipeInfo on both handles.
            assert len(capacities) == 4 and max(capacities) > 0
            requests = COUNT * 41
            responses = COUNT * (40 + len(' blob 4136\n') + 4136 + 1)
            assert requests > max(capacities) and responses > max(capacities)
            row.update(request_bytes=requests, response_bytes=responses,
                       pipe_buffer_sizes=capacities, ordered_bytes=len(actual),
                       digest_sha256=hashlib.sha256(actual).hexdigest())
        result.append(row)
    return {'cases': result, 'production_readers_extracted_verbatim': True,
            'digest_scope': 'unchanged sorted path framing recipe; no full-tree source attestation claim'}


RUST = r'''
use std::path::Path;
use std::process::{Command, Stdio};
use std::io::{BufRead, Write};
use std::ffi::c_void;

/* PRODUCTION_READERS */

#[link(name = "kernel32")]
extern "system" {
    fn GetStdHandle(which: u32) -> *mut c_void;
    fn GetNamedPipeInfo(h: *mut c_void, flags: *mut u32, output: *mut u32,
                        input: *mut u32, max_instances: *mut u32) -> i32;
}

fn fake_git() {
    let mut capacities = Vec::new();
    for which in [-10_i32, -11_i32] {
        let (mut output, mut input) = (0, 0);
        assert_ne!(unsafe { GetNamedPipeInfo(GetStdHandle(which as u32),
            std::ptr::null_mut(), &mut output, &mut input, std::ptr::null_mut()) }, 0);
        capacities.extend([output, input]);
    }
    std::fs::write("capacities.txt", capacities.iter().map(u32::to_string)
        .collect::<Vec<_>>().join(" ")).unwrap();
    let outcome = std::env::var("G115_FAKE_GIT_OUTCOME").unwrap();
    if outcome == "writer-failure" { std::process::exit(17); }
    let mut output = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let id = line.unwrap();
        let i = usize::from_str_radix(&id, 16).unwrap();
        assert!(i < 4096);
        writeln!(output, "{id} blob 4136").unwrap();
        output.write_all(id.as_bytes()).unwrap();
        output.write_all(&vec![(i % 251) as u8; 4096]).unwrap();
        output.write_all(b"\n").unwrap();
        output.flush().unwrap();
    }
    if outcome == "child-failure" { std::process::exit(19); }
}

fn frame(output: &mut Vec<u8>, bytes: &[u8]) {
    output.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    output.extend_from_slice(bytes);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("cat-file") {
        assert_eq!(args.get(2).map(String::as_str), Some("--batch"));
        fake_git();
        return;
    }
    assert_eq!(args.len(), 4);
    let (site, outcome, root) = (&args[1], &args[2], Path::new(&args[3]));
    assert!(matches!(site.as_str(), "build" | "runtime"));
    assert!(matches!(outcome.as_str(), "success" | "writer-failure" | "child-failure"));
    std::env::set_current_dir(root).unwrap();
    std::env::set_var("G115_FAKE_GIT_OUTCOME", outcome);
    // Put the fixture's directory first for the real Command::new("git") calls.
    let executable = std::env::current_exe().unwrap();
    let mut paths = vec![executable.parent().unwrap().to_path_buf()];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    std::env::set_var("PATH", std::env::join_paths(paths).unwrap());
    let mut entries: Vec<GitTreeEntry> = (0..4096).rev().map(|i| GitTreeEntry {
        mode: b"100644".to_vec(), kind: b"blob".to_vec(),
        object_id: format!("{i:040x}"), path: format!("{i:05}").into_bytes(),
    }).collect();
    entries.push(GitTreeEntry { mode: b"160000".to_vec(), kind: b"commit".to_vec(),
        object_id: "f".repeat(40), path: b"z-gitlink".to_vec() });
    entries.sort_by(|a,b| a.path.cmp(&b.path));
    let result: Result<Vec<Option<Vec<u8>>>, String> = if site == "build" {
        std::panic::catch_unwind(|| git_blob_contents(root, &entries)).map_err(|error| {
            error.downcast_ref::<String>().cloned().or_else(||
                error.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
        })
    } else {
        let runtime: Vec<GitTreeEntryV3> = entries.iter().map(|e| GitTreeEntryV3 {
            mode: e.mode.clone(), kind: e.kind.clone(), object_id: e.object_id.clone(),
            path: e.path.clone(),
        }).collect();
        runtime_git_blob_contents(&runtime)
    };
    if outcome == "success" {
        let contents = result.unwrap();
        assert_eq!(contents.len(), entries.len());
        let mut output = b"git-ls-tree-r-z-path-mode-type-framed-blob-content-or-gitlink-oid-sha256/v1\0".to_vec();
        output.extend_from_slice(&(entries.len() as u64).to_be_bytes());
        for (e, content) in entries.iter().zip(contents.iter()) {
            frame(&mut output, &e.path);
            frame(&mut output, &e.mode);
            frame(&mut output, &e.kind);
            frame(&mut output, content.as_deref().unwrap_or(e.object_id.as_bytes()));
        }
        std::fs::write("frames.bin", output).unwrap();
    } else {
        let error = result.unwrap_err();
        let expected = match (site.as_str(), outcome.as_str()) {
            ("build", "writer-failure") => "git cat-file accepts tracked blob ids",
            ("runtime", "writer-failure") => "failed to request a tracked blob",
            ("build", "child-failure") => "git cat-file failed for tracked-tree binding",
            ("runtime", "child-failure") => "git cat-file failed for tracked tree",
            _ => unreachable!(),
        };
        assert!(error.contains(expected), "unexpected failure: {error}");
    }
    std::fs::write("passed.txt", format!("{site} {outcome}")).unwrap();
    println!("OK {site} {outcome}");
}
'''
