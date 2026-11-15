# Frozen Full English Breakfast JSONL v1

`kernel_limited_env --premodern-feb-v1` is available with `eternal-fixtures`. It uses the existing UTF-8 JSONL request bound and flushes one reply per request. The response protocol is `kernel_premodern_feb_jsonl`, schema version `1`, and profile `mtg-kernel-premodern-feb-ordered/v1`.

Every request has `request_type`, `schema_version`, and a nonempty string `request_id` of at most 256 bytes. Each command accepts these additional fields.

| Command | Additional fields |
| --- | --- |
| `reset` | `expected_profile`, `expected_card_db_hash`, `episode_id`, `seed`, `starting_player` (`p0` or `p1`) |
| `current` | None |
| `step` | The issued `prompt`, `native_ordinal`, and `submission` (`human` or `numeric`) |
| `snapshot` | None |
| `restore` | `snapshot_id` |

Reset always creates the frozen 60-card mirror. The request schema rejects extra fields, including deck lists, raw game state, and variant rules. The current catalog hash and profile must match the reset request. `FebSessionV1` owns every game transition.

A successful reply has `response_type` `state` and a `state` object. Every state includes `episode_id` and `authority_generation`. A `decision` carries `prompt` (`episode_id`, `authority_generation`, `actor`, `prompt_seq`) and the same actor's `numeric` and `human` views. A numeric tree has 12 unsigned 16-bit integer limbs per row and two four-limb arrays per topology entry. Both action arrays follow the native ordinal order. Human selection refuses a row marked `unsupported`. A natural `terminal` reports the winner or `null` for a draw. `native_halt` and `unsupported` are separate states. Errors use `response_type` `error` and a stable `code`.

An accepted step returns a `receipt` with its `prompt`, `native_ordinal`, and `submission`. The most recent accepted step can be retried with the same fields within its authority generation. The receipt repeats without another transition. Changing its ordinal or submission conflicts, and a different actor or stale generation is refused. The server also retains the most recent mutating exchange so an exact retransmission with the same request ID repeats its original reply. Use unique request IDs for new requests.

Snapshot handles are opaque 32-character lowercase hexadecimal lookup values namespaced to the server. Up to 64 snapshots belong to the current owner. Reset clears that table, and reset and restore both advance the authority generation. A restored position can reissue the same native step under the new generation. The server keeps the native snapshots, decision tokens, and game state private.

`python/tools/premodern_feb_session_v1.py` provides `FebClientV1`. Pass `[binary, "--premodern-feb-v1"]` and `expected_card_db_hash`, then use `reset`, `current`, `step`, `snapshot`, and `restore`. The client reuses the Limited process and timeout handling.

For native client qualification, set `MTG_KERNEL_FEB_ENV_BIN` to the qualified `kernel_limited_env` binary and `MTG_KERNEL_FEB_CARD_DB_HASH` to its exact catalog hash in decimal or `0x` hexadecimal. Run `python -m unittest discover -s python/tests -p test_premodern_feb_session_v1.py` and require `test_native_opening_reset_step_snapshot_restore_both_seats` to pass.

The process is a trusted host for both seats. It returns only the current actor's view on each decision and has no remote seat authentication. The transport has the same finite FEB scope as the in-process owner.
