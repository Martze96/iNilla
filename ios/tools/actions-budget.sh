#!/usr/bin/env bash
# iNilla: how much of this month's GitHub Actions allowance is spent — for a private repository,
# where macOS minutes count 10× (Windows 2×, Linux 1×) against the plan's included minutes.
#
#   GH_TOKEN=<token> OWNER=<user> [QUOTA=2000] [WARN_AT=60] [FAIL_AT=] ios/tools/actions-budget.sh
#
# GH_TOKEN: a fine-grained personal access token with the account permission "Plan: read-only"
# (stored as the repository secret BILLING_TOKEN). QUOTA: the plan's included minutes (Free 2000,
# Pro 3000). Prints a summary, writes it to the job summary, emits a GitHub warning at WARN_AT %,
# and exits 1 at FAIL_AT % when that is set (the weekly check uses it so GitHub emails you).
set -euo pipefail

QUOTA="${QUOTA:-2000}"
WARN_AT="${WARN_AT:-60}"
FAIL_AT="${FAIL_AT:-}"
OWNER="${OWNER:?set OWNER}"

if [ -z "${GH_TOKEN:-}" ]; then
    echo "::notice::No BILLING_TOKEN secret: skipping the Actions-minutes check (see docs/IOS-BUILD.md)."
    exit 0
fi

year="$(date -u +%Y)"
month="$(date -u +%-m)"
if [ -n "${INILLA_BUDGET_JSON:-}" ]; then
    raw="$INILLA_BUDGET_JSON" # a canned response, for testing this script
else
    raw="$(gh api -H 'Accept: application/vnd.github+json' \
        "/users/${OWNER}/settings/billing/usage?year=${year}&month=${month}" 2>/dev/null || true)"
fi
legacy=""
if [ -z "$raw" ] || ! echo "$raw" | python3 -c 'import json,sys; json.load(sys.stdin)["usageItems"]' >/dev/null 2>&1; then
    legacy="$(gh api "/users/${OWNER}/settings/billing/actions" 2>/dev/null || true)"
fi

summary="$(RAW="$raw" LEGACY="$legacy" QUOTA="$QUOTA" python3 - <<'PY'
import json, os, re
quota = float(os.environ["QUOTA"])
raw, legacy = os.environ.get("RAW", ""), os.environ.get("LEGACY", "")
lin = win = mac = 0.0
if raw:
    try:
        for it in json.loads(raw).get("usageItems", []):
            if it.get("product", "").lower() != "actions" or it.get("unitType", "").lower() not in ("minutes", "minute"):
                continue
            sku = it.get("sku", "").lower()
            q = float(it.get("quantity", 0))
            if "mac" in sku:
                mac += q
            elif "windows" in sku:
                win += q
            else:
                lin += q
    except Exception:
        raw = ""
if not raw and legacy:
    j = json.loads(legacy)
    b = j.get("minutes_used_breakdown", {})
    lin, win, mac = float(b.get("UBUNTU", 0)), float(b.get("WINDOWS", 0)), float(b.get("MACOS", 0))
    quota = float(j.get("included_minutes", quota)) or quota
weighted = lin + 2 * win + 10 * mac
pct = 100.0 * weighted / quota if quota else 0.0
print(f"{pct:.0f}|{weighted:.0f}|{quota:.0f}|{lin:.0f}|{win:.0f}|{mac:.0f}")
PY
)"
IFS='|' read -r pct weighted quota lin win mac <<<"$summary"

line="Actions minutes this month: ${weighted} of ${quota} included (${pct} %) — Linux ${lin}, Windows ${win}×2, macOS ${mac}×10"
echo "$line"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
    {
        echo "### Actions budget"
        echo "$line"
        if [ "$pct" -ge "$WARN_AT" ]; then
            echo ""
            echo "**Over ${WARN_AT} %.** Making the repository public makes standard runners free (Settings → General → Danger Zone → Change visibility)."
        fi
    } >>"$GITHUB_STEP_SUMMARY"
fi
if [ "$pct" -ge "$WARN_AT" ]; then
    echo "::warning::iNilla: ${pct} % of this month's included Actions minutes used (${weighted}/${quota}). Consider making the repository public."
fi
if [ -n "$FAIL_AT" ] && [ "$pct" -ge "$FAIL_AT" ]; then
    exit 1
fi
