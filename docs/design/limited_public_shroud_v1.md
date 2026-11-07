# Public shroud, Limited schema 5

Status: implemented for issue #460 (Premodern Enchantress).

The schema 4 policy observation has no field for shroud, so it refuses a game once a
permanent or a player has shroud. Schema 5 is schema 4 plus an opt-in public shroud
contract:

- `kernel_limited_env --public-shroud-v1` serves `schema_version: 5`, and every reply
  carries `observation_rules: public_shroud_v1`.
- Reset sets `GameState::public_shroud_v1` (`rl::enable_public_shroud_v1`). The flag is
  absent from state bytes and hashes when false, so earlier modes keep theirs.
- Each policy observation of such a game has `projection.public_shroud`:
  `permanents`, the battlefield permanents with shroud (seat 0's battlefield first, in
  public battlefield order), and `players`, each seat's shroud (Solitary Confinement).
  Shroud from any source counts: printed (Argothian Enchantress), granted (Sterling Grove,
  Sylvan Safekeeper's ability).
- Only this observation lifts the refusal. Every other consumer of the same game (the flat
  policy encoders, earlier observation versions) still refuses shroud, so frozen encoders
  and goldens are unchanged.

Schema 4 never represented a player's shroud either. Its refusal now covers Solitary
Confinement's player shroud as well as a permanent's, so a schema 4 consumer never
receives a state it cannot describe.

Verification: `tests/eternal/eternal_public_shroud_v1.rs` plays Argothian Enchantress and
Solitary Confinement games through both schemas (schema 5 reports the shroud, schema 4
halts at the same step with its historical bytes), and the eternal catalog games run on
schema 5.
