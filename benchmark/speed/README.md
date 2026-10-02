# Speed benchmark

How long piiflow and Privado's open-source scanner take, and how much memory they use, on the
twelve applications of the [benchmark corpus](../SELECTION.md). It is the short, always-current
companion to the [labelled benchmark](../PROTOCOL.md), which measures whether the tools are
right; this one measures only cost. The README's "How fast is it?" section is generated from it.

## Method

- **Same machine for both.** Each application gets its own GitHub-hosted runner
  (`ubuntu-latest`), and both tools run on it one after the other. The runner's CPU and memory
  are recorded with the results.
- **piiflow** is the released `x86_64-unknown-linux-musl` binary, downloaded and checked with
  `gh attestation verify` like any user's. It runs three times with the benchmark's own
  invocation (`run.py`); the median wall time is reported, and peak memory is the largest
  maximum resident set size `/usr/bin/time` reports. Its output must equal the recorded
  benchmark run byte for byte, which also checks that Linux gives the same results as macOS.
- **Privado** runs once, exactly as [PROTOCOL.md §8](../PROTOCOL.md#8-comparison-with-privado)
  fixes it: privado-core 1.1.175 from the pinned image, rules v1.3.91, no network, 14 GiB
  memory limit, the same files excluded. Its image is pulled before timing starts; the JVM's
  start-up is part of its time, because it is part of every run. Memory is what Docker reports
  for the container, read once a second, so short peaks can be missed. A run still going after
  an hour is stopped and shown as not finished, with the time it ran; piiflow's numbers are saved
  before Privado starts, so they survive whatever happens to Privado's run.
- **Not in the numbers:** fetching the applications, pulling the image, and the CPU model
  (GitHub assigns runners from a pool; the model is recorded per job).

## Run it

Actions → `benchmark-speed` → Run workflow (or `gh workflow run benchmark-speed.yml -f
version=v0.1.0`). Download the `speed-results` artifact and commit what it holds, or rebuild
it from the per-application artifacts:

```bash
python3 benchmark/speed/collect.py <folder with the <app>.json files> --write
```

That writes `results.json`, `chart.svg` and the README section between the
`<!-- speed:start -->` and `<!-- speed:end -->` markers.
