"""Local XMage observation adapter. The only inference backend is native Net8 CPU.

This process never advances either rules engine. Journals are for retrospective
analysis and contain the agent's private observation, so do not display them to
the human opponent during play.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import threading
import time
import uuid

MAX_LINE = 16 * 1024 * 1024
FLOAT_FIELDS = {"state", "object_features", "edge_features", "action_features", "action_ref_features"}
TENSOR_FIELDS = (
    "state", "object_features", "object_card_ids", "object_groups", "object_node_ids",
    "edge_features", "edge_source_indices", "edge_target_indices", "action_features",
    "action_ref_features", "action_ref_card_ids", "action_ref_action_indices", "action_ref_node_indices",
)


def canonical(value):
    return json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":"), allow_nan=False)


def digest(data):
    return hashlib.sha256(data).hexdigest()


class NativeClient:
    def __init__(self, executable, config, journal, timeout):
        self.timeout = timeout
        self.journal = journal
        self.lines = queue.Queue(maxsize=2)
        self.stderr = (journal / "native-stderr.log").open("xb")
        self.process = subprocess.Popen(
            [str(executable), "--config", str(config)], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=self.stderr,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
        )
        threading.Thread(target=self._read, daemon=True).start()
        try:
            self.ready = self.receive()
            if self.ready.get("schema") != "mtg-kernel-xmage-observed-inference-ready/v1":
                raise ValueError("native service did not identify the observed inference protocol")
        except BaseException:
            self.close()
            raise

    def _read(self):
        while True:
            line = self.process.stdout.readline(MAX_LINE + 1)
            self.lines.put(line)
            if not line or len(line) > MAX_LINE:
                return

    def receive(self):
        try:
            raw = self.lines.get(timeout=self.timeout)
        except queue.Empty as error:
            raise TimeoutError("native inference timed out") from error
        if not raw or len(raw) > MAX_LINE or not raw.endswith(b"\n"):
            raise ValueError("native inference returned an incomplete or oversized response")
        response = json.loads(raw)
        if not isinstance(response, dict):
            raise ValueError("native inference response must be an object")
        return response

    def score(self, request):
        raw = canonical(request).encode("utf-8")
        if len(raw) + 1 > MAX_LINE:
            raise ValueError("encoded decision exceeds transport size limit")
        self.process.stdin.write(raw + b"\n")
        self.process.stdin.flush()
        response = self.receive()
        if "error" in response or response.get("schema", "").endswith("error/v1"):
            raise ValueError("native service rejected the observed decision: " + canonical(response))
        for key in ("request_id", "session_id", "decision_id"):
            if response.get(key) != request[key]:
                raise ValueError("native response binding mismatch: " + key)
        # The native service binds exact request bytes, excluding the newline.
        expected_sha = digest(raw)
        actual_sha = response.get("request_sha256", response.get("input_sha256"))
        if actual_sha != expected_sha:
            raise ValueError("native response request digest mismatch")
        selected = response.get("selected_action_index")
        if type(selected) is not int or not 0 <= selected < len(request["action_ids"]):
            raise ValueError("native response action index mismatch")
        if response.get("selected_action_id") != request["action_ids"][selected]:
            raise ValueError("native response action identity mismatch")
        return response

    def close(self):
        try:
            if self.process.stdin:
                self.process.stdin.close()
            self.process.wait(timeout=3)
        except (OSError, subprocess.TimeoutExpired):
            self.process.kill()
            self.process.wait(timeout=3)
        finally:
            self.stderr.close()


def tensor_wire(encoded, torch):
    result = {}
    for name in TENSOR_FIELDS:
        tensor = getattr(encoded, name).detach().cpu().contiguous().reshape(-1)
        if name in FLOAT_FIELDS:
            if tensor.dtype != torch.float32 or not torch.isfinite(tensor).all().item():
                raise ValueError("invalid encoded float tensor " + name)
            result[name] = [value & 0xffffffff for value in tensor.view(torch.int32).tolist()]
        else:
            if tensor.dtype != torch.int64:
                raise ValueError("invalid encoded index tensor " + name)
            result[name] = tensor.tolist()
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native", required=True, type=Path)
    parser.add_argument("--native-config", required=True, type=Path)
    parser.add_argument("--journal-dir", required=True, type=Path)
    parser.add_argument("--timeout", type=float, default=30)
    args = parser.parse_args()
    if not 1 <= args.timeout <= 120:
        parser.error("timeout must be between 1 and 120 seconds")
    # CPU-only PyTorch is used to encode existing features, never to select an action.
    os.environ["CUDA_VISIBLE_DEVICES"] = ""
    import torch
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    from mtg_kernel_rl import features_v6
    from mtg_kernel_rl.xmage_observation_v1 import project_request

    journal_dir = args.journal_dir.resolve() / (time.strftime("%Y%m%dT%H%M%S") + "-" + uuid.uuid4().hex)
    journal_dir.mkdir(parents=True, exist_ok=False)
    contract_path = Path(__file__).resolve().parents[2] / "data/flat_policy_v3/feature_contract_v3.json"
    contract = json.loads(contract_path.read_text(encoding="utf-8"))
    if digest(Path(features_v6.__file__).read_bytes()) != contract["features_source_sha256"]:
        raise ValueError("feature encoder source does not match current contract")
    client = None
    last_request = None
    last_response = None
    last_raw = None
    sequence = 0
    with (journal_dir / "events.jsonl").open("x", encoding="utf-8") as journal:
        def record(value):
            journal.write(canonical(value) + "\n")
            journal.flush()
        try:
            client = NativeClient(args.native.resolve(), args.native_config.resolve(), journal_dir, args.timeout)
            for key in ("feature_contract_digest", "feature_encoding_digest"):
                if client.ready.get(key) != contract[key]:
                    raise ValueError("native and Python feature identities disagree")
            record({"event": "ready", "native": client.ready,
                    "native_executable_sha256": digest(args.native.read_bytes()),
                    "native_config_sha256": digest(args.native_config.read_bytes()),
                    "projection": "xmage-observed-v1", "training": False,
                    "thinking_engine": "native-cpu-forward", "xmage_simulations": False})
            while True:
                raw = sys.stdin.buffer.readline(MAX_LINE + 1)
                if not raw:
                    record({"event": "closed", "decisions": sequence})
                    return 0
                if len(raw) > MAX_LINE or not raw.endswith(b"\n"):
                    raise ValueError("XMage request exceeds framing limit")
                incoming = json.loads(raw)
                if not isinstance(incoming, dict) or not isinstance(incoming.get("request_id"), str):
                    raise ValueError("XMage request is missing its identity")
                request_id = incoming["request_id"]
                if request_id == last_request:
                    if raw != last_raw:
                        raise ValueError("conflicting repeated XMage request")
                    print(canonical(last_response), flush=True)
                    continue
                started = time.perf_counter()
                observation, legal_actions, diagnostics = project_request(incoming)
                encoded = features_v6.encode_decision(observation, legal_actions)
                sequence += 1
                native_request = {
                    "schema": "mtg-kernel-xmage-observed-decision/v1",
                    "request_id": request_id, "session_id": incoming["game_id"],
                    "decision_id": sequence,
                    "feature_contract_digest": contract["feature_contract_digest"],
                    "feature_encoding_digest": contract["feature_encoding_digest"],
                    "action_ids": [action["stable_id"] for action in legal_actions],
                    "tensor": tensor_wire(encoded, torch),
                }
                scored = client.score(native_request)
                index = scored["selected_action_index"]
                response = {"request_id": request_id, "selected_index": index,
                            "diagnostics": diagnostics}
                record({"event": "decision", "sequence": sequence, "request": incoming,
                        "projection_diagnostics": diagnostics, "observation": observation,
                        "legal_actions": legal_actions, "native": scored,
                        "elapsed_seconds": time.perf_counter() - started})
                print(canonical(response), flush=True)
                last_request, last_response, last_raw = request_id, response, raw
        except Exception as error:
            record({"event": "adapter_error", "error_type": type(error).__name__, "detail": str(error)})
            print(canonical({"request_id": locals().get("request_id", "startup"),
                             "error": "Observation/action mismatch or unsupported decision; evaluation stopped.",
                             "diagnostic_path": str(journal_dir)}), flush=True)
            return 1
        finally:
            if client is not None:
                client.close()


if __name__ == "__main__":
    raise SystemExit(main())
