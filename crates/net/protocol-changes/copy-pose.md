`Command::CopyPose(Option<CopyPose>)`: where the copy the player places
stands (pivot, turn, mirrored, upside down), or that it went away, so its
Add-On can show the others the box round it (`on_copy_ghost`, the New
Duplicator's blue highlight box). Sent at most ten times a second, under
the ghost brick's report limit.
