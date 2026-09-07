# Recipes used to show off nested modules

# Print a greeting
#
# Demonstrates parameters with defaults and a variadic tail.
[group('demo')]
greet name="world" *rest:
    echo "hello {{name}} {{rest}}"

# A recipe with a shebang body
[group('demo')]
report:
    #!/usr/bin/env bash
    set -euo pipefail
    for i in 1 2 3; do
      echo "line $i"
    done

# Runs its dependency first
[group('demo')]
pipeline: greet
    echo "done"
