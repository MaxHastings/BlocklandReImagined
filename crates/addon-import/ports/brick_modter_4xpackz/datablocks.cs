// Brick_ModTer_4xPackZ picks each brick's category at load:
// "ModTer" only when Brick_Modified_ModTer is installed, turned on and its
// CategoryChange preference set, else "Baseplates". That Add-On is not
// bundled, so v20 loads every brick under Baseplates. Here they are declared
// with that category; every other field is the copy's, pinned by entry.json.

datablock fxDTSBrickData(brick4Cube1ModTerData)
{
	brickFile = "Add-Ons/Brick_ModTer_4xPackZ/Bricks/4c.blb";
	category = "Baseplates";
	subCategory = "ModTer 4x";
	uiName = "4x Cube - No Overlap";
	iconName = "Add-ons/Brick_ModTer_4xPackZ/BrickIcons/4c";
	collisionShapeName = "Add-Ons/Brick_ModTer_4xPackZ/Shapes/4cCol.dts";
	hasPrint = 1;
	isWaterBrick = 1;
	printAspectRatio = "ModTer";
};
datablock fxDTSBrickData(brick4Wedge1ModTerData)
{
	brickFile = "Add-Ons/Brick_ModTer_4xPackZ/Bricks/4cW.blb";
	category = "Baseplates";
	subCategory = "ModTer 4x";
	uiName = "4x Wedge - No Overlap";
	iconName = "Add-ons/Brick_ModTer_4xPackZ/BrickIcons/4cW";
	collisionShapeName = "Add-Ons/Brick_ModTer_4xPackZ/Shapes/4cWCol.dts";
	hasPrint = 1;
	isWaterBrick = 1;
	printAspectRatio = "ModTer";
};
datablock fxDTSBrickData(brick4Ramp1ModTerData)
{
	brickFile = "Add-Ons/Brick_ModTer_4xPackZ/Bricks/4cR.blb";
	category = "Baseplates";
	subCategory = "ModTer 4x";
	uiName = "4x Ramp - No Overlap";
	iconName = "Add-ons/Brick_ModTer_4xPackZ/BrickIcons/4cR";
	collisionShapeName = "Add-Ons/Brick_ModTer_4xPackZ/Shapes/4cRCol.dts";
	hasPrint = 1;
	isWaterBrick = 1;
	printAspectRatio = "ModTer";
};
datablock fxDTSBrickData(brick4Ramp1invModTerData)
{
	brickFile = "Add-Ons/Brick_ModTer_4xPackZ/Bricks/4cRinv.blb";
	category = "Baseplates";
	subCategory = "ModTer 4x Inv";
	uiName = "4x Ramp Inv. - No Overlap";
	iconName = "Add-ons/Brick_ModTer_4xPackZ/BrickIcons/4cRinv";
	collisionShapeName = "Add-Ons/Brick_ModTer_4xPackZ/Shapes/4cRinvCol.dts";
	hasPrint = 1;
	isWaterBrick = 1;
	printAspectRatio = "ModTer";
};
datablock fxDTSBrickData(brick4CornerA1ModTerData)
{
	brickFile = "Add-Ons/Brick_ModTer_4xPackZ/Bricks/4cCA.blb";
	category = "Baseplates";
	subCategory = "ModTer 4x";
	uiName = "4x CornerA - No Overlap";
	iconName = "Add-ons/Brick_ModTer_4xPackZ/BrickIcons/4cCA";
	collisionShapeName = "Add-Ons/Brick_ModTer_4xPackZ/Shapes/4cCACol.dts";
	hasPrint = 1;
	isWaterBrick = 1;
	printAspectRatio = "ModTer";
};
datablock fxDTSBrickData(brick4CornerA1invModTerData)
{
	brickFile = "Add-Ons/Brick_ModTer_4xPackZ/Bricks/4cCAinv.blb";
	category = "Baseplates";
	subCategory = "ModTer 4x Inv";
	uiName = "4x CornerA Inv. - No Overlap";
	iconName = "Add-ons/Brick_ModTer_4xPackZ/BrickIcons/4cCAinv";
	collisionShapeName = "Add-Ons/Brick_ModTer_4xPackZ/Shapes/4cCAinvCol.dts";
	hasPrint = 1;
	isWaterBrick = 1;
	printAspectRatio = "ModTer";
};
datablock fxDTSBrickData(brick4CornerB1ModTerData)
{
	brickFile = "Add-Ons/Brick_ModTer_4xPackZ/Bricks/4cCB.blb";
	category = "Baseplates";
	subCategory = "ModTer 4x";
	uiName = "4x CornerB - No Overlap";
	iconName = "Add-ons/Brick_ModTer_4xPackZ/BrickIcons/4cCB";
	collisionShapeName = "Add-Ons/Brick_ModTer_4xPackZ/Shapes/4cCBCol.dts";
	hasPrint = 1;
	isWaterBrick = 1;
	printAspectRatio = "ModTer";
};
datablock fxDTSBrickData(brick4CornerB1invModTerData)
{
	brickFile = "Add-Ons/Brick_ModTer_4xPackZ/Bricks/4cCBinv.blb";
	category = "Baseplates";
	subCategory = "ModTer 4x Inv";
	uiName = "4x CornerB Inv. - No Overlap";
	iconName = "Add-ons/Brick_ModTer_4xPackZ/BrickIcons/4cCBinv";
	collisionShapeName = "Add-Ons/Brick_ModTer_4xPackZ/Shapes/4cCBinvCol.dts";
	hasPrint = 1;
	isWaterBrick = 1;
	printAspectRatio = "ModTer";
};
datablock fxDTSBrickData(brick4CornerC1ModTerData)
{
	brickFile = "Add-Ons/Brick_ModTer_4xPackZ/Bricks/4cCC.blb";
	category = "Baseplates";
	subCategory = "ModTer 4x";
	uiName = "4x CornerC - No Overlap";
	iconName = "Add-ons/Brick_ModTer_4xPackZ/BrickIcons/4cCC";
	collisionShapeName = "Add-Ons/Brick_ModTer_4xPackZ/Shapes/4cCCCol.dts";
	hasPrint = 1;
	isWaterBrick = 1;
	printAspectRatio = "ModTer";
};
datablock fxDTSBrickData(brick4CornerC1invModTerData)
{
	brickFile = "Add-Ons/Brick_ModTer_4xPackZ/Bricks/4cCCinv.blb";
	category = "Baseplates";
	subCategory = "ModTer 4x Inv";
	uiName = "4x CornerC Inv. - No Overlap";
	iconName = "Add-ons/Brick_ModTer_4xPackZ/BrickIcons/4cCCinv";
	collisionShapeName = "Add-Ons/Brick_ModTer_4xPackZ/Shapes/4cCCinvCol.dts";
	hasPrint = 1;
	isWaterBrick = 1;
	printAspectRatio = "ModTer";
};
datablock fxDTSBrickData(brick4CornerD1ModTerData)
{
	brickFile = "Add-Ons/Brick_ModTer_4xPackZ/Bricks/4cCD.blb";
	category = "Baseplates";
	subCategory = "ModTer 4x";
	uiName = "4x CornerD - No Overlap";
	iconName = "Add-ons/Brick_ModTer_4xPackZ/BrickIcons/4cCD";
	collisionShapeName = "Add-Ons/Brick_ModTer_4xPackZ/Shapes/4cCDCol.dts";
	hasPrint = 1;
	isWaterBrick = 1;
	printAspectRatio = "ModTer";
};
datablock fxDTSBrickData(brick4CornerD1invModTerData)
{
	brickFile = "Add-Ons/Brick_ModTer_4xPackZ/Bricks/4cCDinv.blb";
	category = "Baseplates";
	subCategory = "ModTer 4x Inv";
	uiName = "4x CornerD Inv. - No Overlap";
	iconName = "Add-ons/Brick_ModTer_4xPackZ/BrickIcons/4cCDinv";
	collisionShapeName = "Add-Ons/Brick_ModTer_4xPackZ/Shapes/4cCDinvCol.dts";
	hasPrint = 1;
	isWaterBrick = 1;
	printAspectRatio = "ModTer";
};
