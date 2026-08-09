# Lunar Magic 3.63 Map16 rectangle gesture oracle

Source program: authenticated `Lunar Magic.exe` 3.63 in the labeled Ghidra project served at
`127.0.0.1:8089`. This is retained static original-program evidence; it does not claim a live Wine
cross-process gesture capture.

`HandleMap16RenderWindow` (`00500850`) routes `WM_LBUTTONDOWN` to
`HandleMap16EditorLeftButtonDown` (`004FBC50`), which begins a new selection when the target is not
already selected. `HandleMap16EditorMouseMove` (`004FB750`) sends active state 1 through
`MoveMap16SelectionAnchorAndRedraw` (`004EB110`). That function snaps both axes to 16-pixel cells,
normalizes old and new rectangles independently, redraws, and calls `ShowMap16SelectionDimensions`
(`004FB620`). The reported dimensions are `abs(endpoint-origin) / 16 + 1` on each axis. Mouse-up or
capture loss reaches `FinishMap16SelectionDrag` (`004FBB10`).

`DrawMap16SelectionMarquee` (`004F9340`) draws each edge in one-source-pixel steps, resetting its
phase for every edge. Its counter begins at one and tests `counter & 3 < 2`, yielding the repeating
source-pixel colors white, black, black, white. Rust applies that phase before display scaling, so
the marquee retains the original source-pixel shape from 100% through 5000% zoom.
