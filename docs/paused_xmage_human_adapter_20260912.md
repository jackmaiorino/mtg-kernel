# Paused XMage human adapter

Jack allowed the native interface if XMage integration was substantial. The native browser interface is the active human-feedback path. A rich XMage observation/action translator was still needed, so XMage did not offer an immediate shortcut.

Preserved native-side prototype files are `xmage_observed_inference_v1.rs`, its binary entrypoint, and `python/tools/xmage_native_observer_v1.py`. The Rust endpoint compiles and accepts explicitly supplied observation tensors. The Python observation projector it imports is not implemented. These files are not a functioning XMage adapter and are not used by the native human match driver.

Partial Java work remains in `E:/mage-native-human-codex`, branch `codex/native-xmage-human-v1`, base5090ae90680b62428e607964fe85d3fd01b06cd4: `ComputerPlayerNative.java`, `NativeObservedProjector.java`, and `NativeObservedTransport.java`. Baseline Maven packaging passed before these additions. The new Java source has not been verified or registered as a playable client opponent. No ready XMage player is claimed.

Do not silently point this prototype at a promoted model or use XMage state to search. A future adapter needs complete actor-visible projection, legal-action translation, round-trip binding, hidden-information checks and explicit synchronization diagnostics. Preserve existing native and external measurements.
