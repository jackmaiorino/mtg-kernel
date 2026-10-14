# Panda session fingerprint cache

Native122 at `f352a4f52dd1996d27c7af9061edee70609d7bac` passes the focused
library run (136 passed, zero failed, one ignored) and reaches the original
Panda Divine session regression. Its real three-target phase reports input
1862 ms, bound revalidation 1863 ms and Step 3640 ms. This is observed session
cost, not attribution of the dominant component or a measured speedup.

Source inspection shows repeated SHA-256 calculation over compiled source
strings in the Panda observation producer and five dependency producers:
Ajani, Faebloom, Hunter, Eaten Alive and Panda facts. Those functions take no
state, episode, seat, model or runtime inputs. Their inputs are immutable schema
strings, embedded source and immutable dependency digests for one build.

Each now retains its first computed digest in a process-local `OnceLock<String>`
and returns a clone. Input ordering, delimiters, hashing and formatting inside
the initializer are unchanged. No observation, action, state, binding or legal
menu is cached. Source identity still changes when its included source changes;
this source edit therefore needs current producer/replay qualification rather
than a claim of prior raw-transcript parity.

Pinned formatting and diff checks pass. Runtime qualification and the full
260-target, both-seat regression remain pending. Its target count, restores at
0/255, bound revalidation and every final result assertion are preserved. The
original Native122 executable and source stay unchanged until completion and
release with outputs retained. No training or playing-strength claim is made.

The continued real session path also builds Fiery, Krenko, Zul Ashur, Banner and
Inspiration observations. Their five zero-argument producer digests each hash
only compiled schema/source strings and are now retained once per process. Their
initializer bodies, input order, delimiters and hex formatting are unchanged.
Akroma's aggregate digest was already cached. Opt-in existing test phase timing
is copied from the qualified focused-source branch; the complete260-target/both-seat
session and all restore/result assertions remain unchanged. Native123 stays at
immutable711 until its original completion/release/output retention. Current
performance and whole-session qualification are not inferred from this source.
