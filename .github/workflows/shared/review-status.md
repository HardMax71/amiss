---
safe-outputs:
  report-failure-as-issue: false
  missing-tool:
    create-issue: false
  report-incomplete:
    create-issue: false
---

Only evidenced repository defects belong in findings or new issues. Insufficient
balance, quota limits, authentication failures, timeouts, unavailable tools, and
provider outages are operational failures, not code defects. If those prevent
completion, use report_incomplete with the reason; do not create an issue, invent
a code finding, or submit a clean-review verdict. Completed, verified findings
may still be reported, but say explicitly which work remains unreviewed.
