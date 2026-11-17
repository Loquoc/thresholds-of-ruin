"""Validate complete disclosed ranged acquisition decision measurements, not launcher pairing."""
from workload_report import main, positive_counts, require, summarize_matrix
COUNTS = ["route_searches", "game_clones", "observation_cells_copied"]
def check_row(row):
    require(row.get("seed") == 42, "fixed seed required")
    positive_counts(row, ["route_searches"])
    require(type(row.get("observation_cells_copied")) is int and row["observation_cells_copied"] >= 0, "observation copy count must be nonnegative")
    require(type(row.get("game_clones")) is int and row["game_clones"] >= 0, "projection clone count must be nonnegative")
    require(row["route_searches"] == 1, "one shared remembered route search required")
def summarize(rows):
    return summarize_matrix(rows, matches=lambda row: row.get("version") == 1,
        key_fields=["mode", "distance", "items"],
        cases={(w, d, n) for w in ("payload", "unmatched") for d in (4, 6) for n in (1, 16, 128)},
        check_row=check_row, sample_ids=range(20), timings=["decision_ms"], counts=COUNTS)
if __name__ == "__main__":
    main(summarize)
