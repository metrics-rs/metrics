#!/usr/bin/env python3
import argparse
from ddsketch.ddsketch import LogCollapsingLowestDenseDDSketch
import numpy as np
import os


def main():
    parser = argparse.ArgumentParser(description='Process some integers.')
    parser.add_argument('input', type=argparse.FileType('r'))
    parser.add_argument('output', type=argparse.FileType('w'))
    parser.add_argument('alpha', type=float, nargs='?', default=0.0001)
    parser.add_argument('max_bins', type=int, nargs='?', default=32768)
    args = parser.parse_args()

    input_floats = []
    for line in args.input.readlines():
        input_floats += [float(i) for i in line.split(",") if i.strip()]

    sketch = LogCollapsingLowestDenseDDSketch(relative_accuracy=args.alpha, bin_limit=args.max_bins)
    for v in input_floats:
        sketch.add(v)
    
    output_quantiles = [(x, sketch.get_quantile_value(x)) for x in np.linspace(0, 1, 1000)]
    for quantile, value in output_quantiles:
        args.output.write(f"{quantile:.3},{value:.9}\n")

    args.output.flush()
    os.fsync(args.output)


if __name__ == "__main__":
    main()
