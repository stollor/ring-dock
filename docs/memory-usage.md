# Ring Dock memory measurements

## Test setup

- Environment: Windows 11 with a 2560×1392 work area.
- The same configuration was used. All four categories were opened and closed in sequence; measurements were taken after two or three cycles.
- The preview comparison uses the program before and after the optimization. The formal build measurement uses the optimized release instance.
- Diagnostic frame output was disabled. Private working set counts resident private process pages using Windows QueryWorkingSet. Private commit and total working set were also recorded.

## Results

| Measurement | Private working set | Private commit | Total working set |
| --- | ---: | ---: | ---: |
| Preview before optimization, closed after 2 cycles | 26.934 MiB | 37.117 MiB | 75.852 MiB |
| Preview after optimization, closed after 2 cycles | 11.859 MiB | 20.914 MiB | 47.410 MiB |
| Preview after optimization, closed after 3 cycles | 12.305 MiB | 21.258 MiB | 47.848 MiB |
| Optimized release instance, closed after 2 cycles | 12.695 MiB | 21.723 MiB | 50.230 MiB |

In the two-cycle preview comparison, private working set fell by about 56% and private commit by about 44%. The formal instance measured 6.070 MiB of private working set immediately after a cold start while closed, then about 12.7 MiB after loading icons and fonts. The cold-start number does not represent typical use.

The work area was 2560×1392. When closed, the transparent window's drawing buffer was about 340×341 pixels, or 0.442 MiB. It grows to fit the panel while open; closing the panel releases the larger software-rendering target. Icons load for visible items so Shell icons for off-screen panel entries do not all need to stay resident.

## Interpreting the numbers

This is a short before-and-after measurement on one PC. Results will vary, and the run is not a long-term memory-leak test. Private working set counts resident private physical pages. Total working set also includes shared Windows pages, so the two metrics are not interchangeable. Windows version, file associations, Shell extensions, display size, and desktop software affect the results.
