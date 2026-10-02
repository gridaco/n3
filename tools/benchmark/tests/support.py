"""Transport-only test payloads; full semantic fixtures live in fixtures/."""

def capture_envelope(options):
    """Minimal envelope for transport tests; not an admissible performance record."""
    return {
        "schema": "n3.viewport-measure.v2",
        "options": options,
        "samples": [{"frame": index, "cpu_frame_ms": 1} for index in range(options["sample_frames"])],
        "validity": {"mode_contract_satisfied": True},
    }
