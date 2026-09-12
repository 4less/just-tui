data    := "data"
results := "results"

# Aligning the reads and counting what mapped

# Align the reads to the reference and count what mapped
#
# minimap2 writes its progress to stderr and the alignments to a file, so the
# job's .err holds the mapping log and its .out holds the flagstat table.
[doc("Align the reads to the reference, then count what mapped")]
[group('2. align')]
reads preset="sr" threads="8":
    mkdir -p {{results}}
    minimap2 -ax {{preset}} -t {{threads}} -o {{results}}/aln.sam \
      {{data}}/phix.fa {{data}}/reads.fq
    samtools flagstat {{results}}/aln.sam

# Sort and index the alignments
[group('2. align')]
sort:
    samtools sort -o {{results}}/aln.bam {{results}}/aln.sam
    samtools index {{results}}/aln.bam

# Depth across the genome, one line per position
[group('3. report')]
depth:
    samtools depth -a {{results}}/aln.bam > {{results}}/depth.tsv
