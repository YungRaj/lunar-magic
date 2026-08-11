# Auto-Deselect on Editor Select provenance

## Original behavior

The authenticated command table maps `LM_OPTIONS_AUTO_DESELECT` `$24DB` to byte `$A4` at image
address `$00498AAE` (`command_id + $004965D3`). `HandleLevelEditorCommand` routes case `$A4`
through the original checked-option toggle without modifying the open ROM.

The Lunar Magic 3.63 CHM `option_general.htm#option_auto_deselect` defines the observable boundary:
when enabled, making a new selection in an Add Object/Sprite window or the Map16 editor deselects
anything selected in the main level editor. Its purpose is to let the next paste use the selector
choice without requiring Control. It does not alter ordinary selection within the main canvas.

## Native publication boundary

Rust exposes the toggle in the normal Tools menu and through the authenticated internal toolbar
command. Its application preference persists independently of the ROM. Standard, extended, and
custom Layer 1/Layer 2 object choices; existing, standard, and custom sprite choices; and Map16
tile/rectangle selections all clear the active Layer 1, Layer 2, and sprite canvas groups when the
option is enabled. With the option disabled the same selector choice preserves every group.

Focused tests bind both toggle directions, persistence/reopen, disabled preservation, enabled
cross-domain clearing through a real extended-object selector choice, and the complete authenticated
command partition.
