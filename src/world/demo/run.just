results := "results"
rgmsa := "../rg-msa/target/release/rg-msa"

# Per-arm stages of the benchmark
#
# Each recipe takes an arm name. The arms are the cells of the experiment:
# focal or non-focal reference, congeneric or target-level filtering.

# rg-msa over one arm's reads
#
# Builds the variable-sites MSA for a single arm. The heavy one: budget
# 64 cores and about a terabyte for the larger arms.
[doc("rg-msa over one arm's reads — the variable-sites MSA")]
[group('2. rg-msa')]
s01_msa arm force="":
    @set -eu; \
    test -x "{{rgmsa}}" || { echo "build it first (cd ../rg-msa && cargo build --release)"; exit 1; }
    printf '%s: msa --gene-selection %s\n' "{{arm}}" "$sel"
    rg-msa msa --arm {{arm}} --out {{results}}/rgmsa/{{arm}}

# Refilter an existing MSA without recomputing it
[doc("rg-msa refilter -- the variable-sites MSA for one arm")]
[group('2. rg-msa')]
s02_refilter arm force="":
    rg-msa refilter --arm {{arm}} --out {{results}}/rgmsa/{{arm}}_filt

# Build the gene tree for one arm
[group('3. trees')]
s01_tree msa out="" threads="8":
    iqtree2 -s {{msa}} -T {{threads}} --prefix {{out}}

# Collect the per-arm metrics into one table
[group('4. report')]
collect:
    rg-msa collect {{results}}/rgmsa -o {{results}}/metrics.tsv
