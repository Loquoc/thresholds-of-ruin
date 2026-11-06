"""Validate complete equipment-decision workload v1 samples."""
from workload_report import main, positive_counts, require, summarize_matrix

COUNTS = ["gear_projections", "game_clones", "dynamic_folds"]

def check_row(row):
    require(row.get("seed") == 42, "fixed seed required")
    require(row.get("selected_item") == 99 + row["items"], "independent highest armor choice required")
    positive_counts(row, COUNTS)
    require(row["game_clones"] >= row["gear_projections"], "each projection must own a Game clone")

def summarize(rows):
    return summarize_matrix(rows, matches=lambda row: row.get("version") == 1,
        key_fields=["items"], cases={(1,), (4,), (16,), (64,)}, check_row=check_row,
        sample_ids=range(20), timings=["decision_ms"], counts=COUNTS)

if __name__ == "__main__":
    main(summarize)
