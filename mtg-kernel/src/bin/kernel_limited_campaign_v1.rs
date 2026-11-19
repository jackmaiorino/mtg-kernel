//! The Foundations Limited training campaign runner of issue 657
//! (`docs/design/limited_campaign_v1.md`). The logic lives in
//! `mtg_kernel::limited_campaign_v1`; this file parses the command line.
//! `kernel_limited_training_v1` (the bounded PR 586 trainer) is unchanged.
use mtg_kernel::limited_campaign_v1::campaign::{
    lock_manifest_v1, manifest_bytes_v1, publish_atomic_v1, qualify_v1, resume_v1, start_v1,
    verify_v1, LaunchV1, ModeV1, RunOutcomeV1,
};
use mtg_kernel::limited_campaign_v1::checkpoint::LearnerKindV1;
use mtg_kernel::limited_campaign_v1::host::measure_host_class_v1;
use mtg_kernel::limited_campaign_v1::inventory::default_repo_root_v1;
use mtg_kernel::limited_campaign_v1::sha256_hex;
use std::path::PathBuf;

mtg_kernel::fast_global_allocator!();

const USAGE: &str = "usage:
  kernel_limited_campaign_v1 train  [--stage-diagnostics] -out DIR -workers W -u-max U [-learner L] [-qualification-manifest PATH] [-host-class NAME]
  kernel_limited_campaign_v1 smoke  [--stage-diagnostics] -out DIR -workers W -u-max 1|2 [-learner L]
  kernel_limited_campaign_v1 resume [--stage-diagnostics] -out DIR -workers W [-qualification-manifest PATH] [-host-class NAME]
  kernel_limited_campaign_v1 qualify [--stage-diagnostics] -out DIR -workers W -host-class NAME [-learner L]
  kernel_limited_campaign_v1 lock-manifest -out PATH -workers W -u-max U RUN_DIR...
  kernel_limited_campaign_v1 verify -out DIR
options for every command but lock-manifest and verify: -repo-root PATH, -git-head SHA
L is v2-h32 (default), v2-h16 or pair-centered-reinforce-v1";

#[derive(Default)]
struct Options {
    stage_diagnostics: bool,
    out: Option<PathBuf>,
    workers: Option<usize>,
    u_max: Option<u64>,
    learner: Option<String>,
    manifest: Option<PathBuf>,
    host_class: Option<String>,
    repo_root: Option<PathBuf>,
    git_head: Option<String>,
    positional: Vec<PathBuf>,
}

fn parse(arguments: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut iterator = arguments.iter();
    while let Some(argument) = iterator.next() {
        let mut value = || {
            iterator
                .next()
                .cloned()
                .ok_or(format!("{argument} needs a value"))
        };
        match argument.as_str() {
            "--stage-diagnostics" => options.stage_diagnostics = true,
            "-out" => options.out = Some(value()?.into()),
            "-workers" => {
                options.workers = Some(value()?.parse().map_err(|_| "-workers takes a count")?)
            }
            "-u-max" => options.u_max = Some(value()?.parse().map_err(|_| "-u-max takes a count")?),
            "-learner" => options.learner = Some(value()?),
            "-qualification-manifest" => options.manifest = Some(value()?.into()),
            "-host-class" => options.host_class = Some(value()?),
            "-repo-root" => options.repo_root = Some(value()?.into()),
            "-git-head" => options.git_head = Some(value()?),
            flag if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
            path => options.positional.push(path.into()),
        }
    }
    Ok(options)
}

fn learner(name: Option<&str>) -> Result<LearnerKindV1, String> {
    match name.unwrap_or("v2-h32") {
        "v2-h32" => Ok(LearnerKindV1::ActorCriticH32),
        "v2-h16" => Ok(LearnerKindV1::ActorCriticH16),
        "pair-centered-reinforce-v1" => Ok(LearnerKindV1::PairCenteredControl),
        other => Err(format!("unknown learner {other:?}")),
    }
}

fn git_head(repo_root: &std::path::Path) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|error| format!("git rev-parse HEAD: {error}; pass -git-head"))?;
    let head = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || head.is_empty() {
        return Err("git rev-parse HEAD failed; pass -git-head".into());
    }
    Ok(head)
}

fn launch(options: &Options) -> Result<LaunchV1, String> {
    let repo_root = options
        .repo_root
        .clone()
        .unwrap_or_else(default_repo_root_v1);
    let git_head = match &options.git_head {
        Some(head) => head.clone(),
        None => git_head(&repo_root)?,
    };
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let binary_sha256 = sha256_hex(
        &std::fs::read(&executable)
            .map_err(|error| format!("{}: {error}", executable.display()))?,
    );
    let host_class = measure_host_class_v1(
        options
            .host_class
            .as_deref()
            .unwrap_or("engineering-unlisted"),
        &binary_sha256,
    )?;
    Ok(LaunchV1 {
        root: options.out.clone().ok_or("-out is required")?,
        repo_root,
        workers: options.workers.ok_or("-workers is required")?,
        stage_diagnostics: options.stage_diagnostics,
        git_head,
        binary_sha256,
        host_class,
        qualification_manifest: options.manifest.clone(),
    })
}

fn run(arguments: &[String]) -> Result<Option<RunOutcomeV1>, String> {
    let (command, rest) = arguments.split_first().ok_or(USAGE)?;
    let options = parse(rest)?;
    if !options.positional.is_empty() && command != "lock-manifest" {
        return Err(USAGE.into());
    }
    match command.as_str() {
        "train" | "smoke" => {
            let mode = if command == "train" {
                ModeV1::Campaign
            } else {
                ModeV1::Smoke
            };
            let u_max = options.u_max.ok_or("-u-max is required")?;
            start_v1(
                &launch(&options)?,
                mode,
                learner(options.learner.as_deref())?,
                u_max,
            )
            .map(Some)
        }
        "resume" => resume_v1(&launch(&options)?).map(Some),
        "qualify" => {
            if options.host_class.is_none() {
                return Err("qualify needs -host-class".into());
            }
            let run = qualify_v1(&launch(&options)?, learner(options.learner.as_deref())?)?;
            println!("{run}");
            Ok(None)
        }
        "lock-manifest" => {
            let out = options.out.ok_or("-out is required")?;
            let vcpus = std::thread::available_parallelism().map_or(0, |count| count.get());
            let manifest = lock_manifest_v1(
                &options.positional,
                options.workers.ok_or("-workers is required")?,
                options.u_max.ok_or("-u-max is required")?,
                vcpus,
            )?;
            let directory = out.parent().map(PathBuf::from).unwrap_or_default();
            let name = out
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("-out names no file")?;
            let digest = publish_atomic_v1(&directory, name, &manifest_bytes_v1(&manifest)?)?;
            println!("{digest}");
            Ok(None)
        }
        "verify" => {
            let head = verify_v1(&options.out.ok_or("-out is required")?)?;
            println!("verified the chain from step 0 to step {head}");
            Ok(None)
        }
        _ => Err(USAGE.into()),
    }
}

fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match run(&arguments) {
        Ok(Some(outcome)) => {
            println!("stop_reason {}", outcome.stop_reason());
            if outcome == RunOutcomeV1::ExcessTruncation {
                std::process::exit(3);
            }
        }
        Ok(None) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
