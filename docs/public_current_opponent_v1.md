# Frozen current-policy opponent in the public collector

An episode may explicitly set `"opponent_kind": {"kind": "current"}` with
`opponent` and `opponent_search` absent. This is a training-only declaration;
ordinary episode consumers and the terminal evaluator do not admit it. An
absent opponent remains an error in the public collector.

The opponent forks the public policy already installed at the start of the
update, including its input-enable flag and projection weights. It has private
encoder, capture and physical-seat sampler state. Both actors use their own
actor-visible input and the existing quantized sampler. Collection completes
before the next optimizer update. Existing fixed-opponent paths are unchanged.

Current-opponent trajectories use
`mtg-kernel-public-input-current-opponent-trajectory/v1` and an opponent record
binding config, full optimizer-state hash and input-enable flag. The run-level
receipt describes the declaration; actual evolving identity is per trajectory.
Opponent rows/public auxiliaries are archived, while `learner_groups` filters
the learner seat before constructing GAE, normalization or loss inputs. Ordinary
replay-audit admission continues to refuse opponent-record trajectories.

This source prerequisite permits a prospective baseline-style 30% current,
50% untouched parent, 20% A48 curriculum. It freezes no schedule, dose or gate
and supplies no playing-strength evidence. CPU fixture execution, actual-parent
validation, full-config serial/parallel qualification, storage and export cost
remain separate work under a bounded manifest.
