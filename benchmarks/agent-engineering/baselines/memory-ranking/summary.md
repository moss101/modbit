# Memory ranking comparison (PX-186)

Corpus `memory-ranking@1.0.0` (sha256 `6329a8439de3`), 12 tasks, 16 gold groups. Both rankings ran every task through the real Core at the same memory budget; each ran twice on independent profiles with identical results.

| ranking | gold groups recalled | recall | injected tokens | budget tokens | recall per 1k tokens |
|---|---|---|---|---|---|
| `keyword-v1` | 16/16 | 1.000 | 9058 | 9600 | 1.766 |
| `recency-importance-diversity-v2` | 14/16 | 0.875 | 9078 | 9600 | 1.542 |

| task | budget | current recalled | current tokens | candidate recalled | candidate tokens |
|---|---|---|---|---|---|
| staging-port | 800 | 1/1 | 769 | 1/1 | 773 |
| release-owner | 800 | 1/1 | 735 | 1/1 | 751 |
| db-config | 800 | 2/2 | 756 | 2/2 | 738 |
| changelog | 800 | 1/1 | 746 | 1/1 | 749 |
| timeout | 800 | 1/1 | 761 | 1/1 | 756 |
| upload-cmd | 800 | 1/1 | 769 | 1/1 | 771 |
| deploy | 800 | 2/2 | 768 | 1/2 | 781 |
| api-version | 800 | 1/1 | 742 | 1/1 | 736 |
| migration | 800 | 1/1 | 767 | 1/1 | 770 |
| release-prep | 800 | 3/3 | 754 | 2/3 | 759 |
| branch | 800 | 1/1 | 755 | 1/1 | 752 |
| items-limit | 800 | 1/1 | 736 | 1/1 | 742 |

Decision: **revert**. Rule: keep the candidate only when, on the same tasks at the same memory token budget, it recalls more gold groups than the current ranking and its recall per thousand injected tokens is not lower; otherwise revert. The Core's default ranking (a pass naming none) is `keyword-v1`.
