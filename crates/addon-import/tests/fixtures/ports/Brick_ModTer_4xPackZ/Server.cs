// CC0 stand-in for Brick_ModTer_4xPackZ (tests/ports.rs): one brick whose
// category is the original's load-time expression.
datablock fxDTSBrickData(brick4Cube1ModTerData)
{
	brickFile = "Add-Ons/Brick_ModTer_4xPackZ/Bricks/4c.blb";
	category = (isFile("Add-ons/Brick_Modified_ModTer.zip") && $Addon__Brick_Modified_ModTer == 1 && $Pref::Server::ModifiedModter::CategoryChange ? "ModTer" : "Baseplates");
	subCategory = "ModTer 4x";
	uiName = "4x Cube - No Overlap";
	hasPrint = 1;
	printAspectRatio = "ModTer";
};
