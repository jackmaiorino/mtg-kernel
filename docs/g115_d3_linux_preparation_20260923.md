# g115 D3 Linux preparation

Linux portability acceptance passes on the same consumed Wildfire/Rally engineering identities. No formal panel, paid allocation, strength estimate or fleet placement decision was made.

Native source remains `6604306cd67ccfb7e558b49b6504e033885fb0a8`. The isolated Ubuntu checkout and target are `/home/user/g115-d3-20260923-002`; the pre-existing Bookworm runtime was inspected but not modified. Rust/Cargo1.94.1, GCC11.4 linker driver, GNU ld2.38 and bundled LLD21.1.8 are recorded in `E:/mtg-g115-lineage-20260923/d3-linux-build-002/toolchain.json`. Four nice10 compiler jobs built offline in149.561 seconds, preserving the32GiB host reserve. Build001 stopped before compilation because the version query omitted rust-lld's `-flavor gnu`; that failure receipt is retained.

Linux binary SHA256: `d6db661dc93dfec96bc4363c57843d027af4efcd0cae030d5e0797e2422164d4`.

`python/tools/g115_d3_payload_v1.py` stages the actual shared-panel g115 and V3 sources, checks the original source pins, preserves all six leaf artifacts byte-for-byte, relocates descriptor paths and changes only `receipt.destination_build_git_head` in the V3 transfer envelope. It leaves both adapters and the null V3 checkpoint unchanged. The resulting envelope hash equals the already accepted Windows envelope. It cannot launch anything.

| Consumed case | Games | Decisions | Windows/Linux bytes |
| --- | ---: | ---: | --- |
| Seat0 baseline | 2 | 209 | Identical |
| Seat1 baseline | 2 | 116 | Identical |
| Seat0 search | 2 | 223 | Identical |
| Seat1 search | 2 | 196 | Identical |
| Seat0 search repeat | 2 | 223 | Identical |

Receipt: `E:/mtg-g115-lineage-20260923/d3-linux-acceptance-001/completion.json`,123.226 seconds total. Semantic SHA256s match the Windows acceptance exactly, including all search records. The repeat retains the same whole-match store hash. These timings are correctness-run observations, not a matched allocation comparison.

The offline bundle includes the binary, pinned Ubuntu loader/libc/libm/libgcc and exact portable model payload. Its explicit loader resolves that library closure successfully. Bundle: `E:/mtg-g115-lineage-20260923/d3-linux-bundle-001/bundle.tar.gz`,30,440,308 bytes, SHA256 `37879c921ac4f239470e6bd800806d1004fad9974bedd98a8b729fac759814f7`. It targets `/workspace/g115-d3`, has not been uploaded and is marked non-launchable. RunPod still needs native replay, representative serial/increasing-parallel throughput, transfer/recovery cost and the existing bounded lease guard before selection. Full2048 request binding and guarded formal dispatch also remain outstanding. Design review and the recorded72-hour threshold are unchanged.
