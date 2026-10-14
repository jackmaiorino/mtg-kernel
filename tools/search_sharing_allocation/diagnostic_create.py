"""One guarded same-name create attempt with bounded sanitized rejection data."""
import argparse
import json
from pathlib import Path
import re
import time
import urllib.error
import urllib.request

import cloud_lease


class DiagnosedProvider(cloud_lease.prepare_lease.Provider):
    control = None

    def _call(self, request):
        try:
            with urllib.request.urlopen(request, timeout=15) as response:
                body = response.read()
                return json.loads(body) if body else {}
        except urllib.error.HTTPError as error:
            status = error.code
            try:
                raw = json.loads(error.read(8192))
            except (ValueError, OSError):
                raw = {}
            finally:
                error.close()
            safe = {}
            if isinstance(raw, dict):
                for field in ("title", "detail", "message", "error", "status"):
                    value = raw.get(field)
                    if isinstance(value, (str, int)):
                        text = str(value).replace(self.key, "[redacted]")[:1024]
                        if re.search(r"RUNPOD_API_KEY|SSH_PUBLIC_KEY|dockerEntrypoint|Bearer", text, re.I):
                            text = "[request details omitted]"
                        safe[field] = text
            cloud_lease.write(self.control / ("create-http-error-" + str(time.time_ns()) + ".json"),
                              {"epoch": time.time(), "http_status": status, "safe_error_fields": safe})
            raise RuntimeError("provider HTTP " + str(status)) from None
        except (OSError, ValueError) as error:
            cloud_lease.write(self.control / ("create-request-error-" + str(time.time_ns()) + ".json"),
                              {"epoch": time.time(), "error_type": type(error).__name__})
            raise RuntimeError("provider request unavailable") from None


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--control", type=Path, required=True)
    parser.add_argument("--execute", action="store_true")
    args = parser.parse_args()
    DiagnosedProvider.control = args.control
    cloud_lease.prepare_lease.Provider = DiagnosedProvider
    print(json.dumps(cloud_lease.create(args.control, args.execute)))
