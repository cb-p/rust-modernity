# Ruvolution
> Analyze the modernity of Rust code bases using various techniques.

## Setup
Before using this tool, you need to expand the code of the `std`, `core` and `alloc` crates:

1. Clone the [official Rust language repository](https://github.com/rust-lang/rust).
2. Use `cd` to change your working directory to the cloned repository.
3. From the directory, execute the `expand-std.sh` file from this repository. (e.g. `$ ../ruvolution/expand-std.sh`)
4. Copy the three generated files (`expanded-{std,core,alloc}.rs`) back to the Ruvolution directory.

## Usage
To see the usage of this tool, use the `--help` argument:

```
$ cargo run --release -- --help
```

> It is recommended to run in release mode to enable all optimizations, as source code analysis is quite expensive.

When successfully analyzing a crate, the resulting CSV file should appear in the `results` folder.

---

For example, to analyze twenty spread out versions of the `tokio` crate, the following command is used:

```
$ cargo run --release -- tokio
```

The resulting metrics are then written to `results/tokio.csv` to be further processed.

## Analysis
The Python notebook `analyze_results.ipynb` is used to plot the resulting metrics to explore the viability of these metrics to measure code modernity.

It does this by reading all CSV files from the `results` folder, and plotting them against time.
