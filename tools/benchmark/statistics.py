"""Pure descriptive statistics for one run and independent-run aggregation.

No I/O, report schema or performance decisions belong here. Missing stage values
stay unavailable; zero durations remain observed data; slow values are retained.
"""

import math
import statistics

def distribution(values):
    retained = sorted(value for value in values if value is not None)
    if not retained:
        return None
    count = len(retained)
    return {
        "count": count,
        "median": statistics.median(retained),
        "p95": retained[math.ceil(count * 0.95) - 1],
        "min": retained[0],
        "max": retained[-1],
    }


def sample_range(values):
    return {"median": statistics.median(values), "min": min(values), "max": max(values)}


def aggregate(distributions):
    present = [value for value in distributions if value is not None]
    if not present:
        return None
    medians = [value["median"] for value in present]
    result = sample_range(medians)
    deviation = statistics.median(abs(value - result["median"]) for value in medians)
    result.update({
        "mad": deviation,
        "relative_mad_percent": 100 * deviation / result["median"] if result["median"] else None,
    })
    return {
        "unit": "ms", "run_count": len(present), "run_medians": result,
        "run_p95": sample_range([value["p95"] for value in present]),
        "run_max": sample_range([value["max"] for value in present]),
    }
