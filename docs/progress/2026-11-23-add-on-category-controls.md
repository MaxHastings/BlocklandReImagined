# 2026-11-23 Add-On category controls

Issue #451 adds shaded list headers, indented package rows and category
checkboxes with a mixed state. A category applies to its full eligible
membership while searching. Individual selection and checkboxes still
use the existing details and toggle paths.

The host computes eligibility through the package library. The category
command stages normal dependency and role decisions before writing the
selection once and persisting accepted choices. Protected base dependencies
cannot be disabled indirectly. The UI asks before category shutdown would
also affect dependent Add-Ons.
