# Comparative Benchmark Report
| Workload | system (Ops/s) | vs System | halo (Ops/s) | vs System | mimalloc (Ops/s) | vs System | snmalloc (Ops/s) | vs System | jemalloc (Ops/s) | vs System |
|---|---|---|---|---|---|---|---|---|---|---|
| alloc_free_16b | 55.29M | **1.00x** | N/A | - | N/A | - | N/A | - | N/A | - |
| alloc_free_1kb | 1179.67M | **1.00x** | N/A | - | N/A | - | N/A | - | N/A | - |
| alloc_free_1mb | 1176.82M | **1.00x** | N/A | - | N/A | - | N/A | - | N/A | - |
| fragmentation_churn | 250 | **1.00x** | N/A | - | N/A | - | N/A | - | N/A | - |
| larson_16_threads | 62 | **1.00x** | N/A | - | N/A | - | N/A | - | N/A | - |
| larson_1_threads | 252 | **1.00x** | N/A | - | N/A | - | N/A | - | N/A | - |
| larson_2_threads | 250 | **1.00x** | N/A | - | N/A | - | N/A | - | N/A | - |
| larson_4_threads | 253 | **1.00x** | N/A | - | N/A | - | N/A | - | N/A | - |
| larson_8_threads | 87 | **1.00x** | N/A | - | N/A | - | N/A | - | N/A | - |
| threadtest_16_threads | 39 | **1.00x** | N/A | - | N/A | - | N/A | - | N/A | - |
| threadtest_2_threads | 68 | **1.00x** | N/A | - | N/A | - | N/A | - | N/A | - |
| threadtest_4_threads | 66 | **1.00x** | N/A | - | N/A | - | N/A | - | N/A | - |
| threadtest_8_threads | 58 | **1.00x** | N/A | - | N/A | - | N/A | - | N/A | - |
| vec_push_1000 | 798.35K | **1.00x** | N/A | - | N/A | - | N/A | - | N/A | - |
