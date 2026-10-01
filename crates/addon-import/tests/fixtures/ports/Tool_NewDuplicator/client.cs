// Stand-in for the Tool_NewDuplicator port tests (CC0): the shapes of the
// client keybinds the port reads. The port's binds, not this script, run.
$ND::Version = "9.9.1";

function ndRegisterKeybinds()
{
   $RemapDivision[$RemapCount] = "New Duplicator";
   $RemapName[$RemapCount] = "Copy Selection (Ctrl C)";
   $RemapCmd[$RemapCount] = "ndInputCopy";
   $RemapCount++;
   $RemapName[$RemapCount] = "Multiselect (Ctrl, Hold to use)";
   $RemapCmd[$RemapCount] = "ndInputMultiSelect";
   $RemapCount++;
   $RemapName[$RemapCount] = "Send /FillBricks (Shift-Ctrl V)";
   $RemapCmd[$RemapCount] = "ndInputFillBricks";
   $RemapCount++;
}

function clientCmdNdEnableKeybinds(%bool)
{
   %map = new ActionMap(ND_KeyMap);
   %map.bind("keyboard", "ctrl c", "ndInputCopy");
   %map.bind("keyboard", "ctrl v", "ndInputPaste");
   %map.bind("keyboard", "ctrl x", "ndInputCut");
   %map.bind("keyboard", "lcontrol", "ndInputMultiSelect");
   %map.bind("keyboard", "shift-ctrl x", "ndInputSuperCut");
   %map.bind("keyboard", "shift-ctrl v", "ndInputFillBricks");
   %map.push();
}

function ndInputMultiSelect(%bool) { commandToServer('ndMultiSelect', %bool); }
