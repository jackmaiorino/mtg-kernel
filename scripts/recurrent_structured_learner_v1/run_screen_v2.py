#!/usr/bin/env python3
"""Run the fresh hard trust-projected recurrent learner screen."""

from __future__ import annotations

import run_screen_v1 as screen
from model_v2 import TrustProjectedActorCritic


screen.SCHEMA = "mtg-kernel-trust-projected-recurrent-structured-learner-screen/v2"
screen.PROFILE_SCHEMA = screen.SCHEMA + ".profile"
screen.EXPECTED_CACHE_SHA256 = (
    "287d509794658bc167a7b61be450fa894d38ad837e7e6b212d49947629d542c6"
)
screen.EXPECTED_CACHE_SCHEMA = None
screen.CORPUS_PAIR_COUNT = 1_024
screen.RecurrentStructuredActorCritic = TrustProjectedActorCritic


if __name__ == "__main__":
    raise SystemExit(screen.main())
