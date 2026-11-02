#!/usr/bin/env bash
set -euo pipefail

printf 'Matrix preparation: %s\nCheck groups: %s\n' "${CI_PLAN_RESULT:-missing}" "${CI_CHECKS_RESULT:-missing}"
[[ "${CI_PLAN_RESULT:-}" == success && "${CI_CHECKS_RESULT:-}" == success ]]
