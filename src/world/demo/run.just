data    := "data"
results := "results"

# Aligning the reads and counting what mapped

# Index the reference so the aligner can load it
[group('2. align')]
index:
    minibwa index {{data}}/phix.fa

# Align the reads to the reference and count what mapped
#
# minibwa writes its progress to stderr and the alignments to stdout, so the
# SAM goes to a file and the job's .err holds the mapping log — while the
# flagstat table that follows lands in its .out.
[doc("Align the reads to the reference, then count what mapped")]
[group('2. align')]
reads threads="8":
    mkdir -p {{results}}
    minibwa map -t {{threads}} {{data}}/phix.fa {{data}}/reads.fq > {{results}}/aln.sam
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
