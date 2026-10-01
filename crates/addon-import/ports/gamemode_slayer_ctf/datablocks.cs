// Slayer CTF makes these at run time in createSlayerCTFDatablocks(): the
// two bricks, and one flag item and image for each of the first ten paint
// colours. Here they are declared as the importer reads them, with one flag
// item and image that take their brick's or carrier's team colour (the
// engine tints each object), so every paint colour has its flag. Values in
// {{double braces}} are read from this copy's scripts.

datablock fxDTSBrickData(brickSlyrCTFFlagData : {{flag_parent}})
{
	uiName = "{{flag_ui}}";
	category = "Slayer";
	subCategory = "CTF";
	indestructable = 1;
};

datablock fxDTSBrickData(brickSlyrCTFFlagReturnData : {{return_parent}})
{
	uiName = "{{return_ui}}";
	category = "Slayer";
	subCategory = "CTF";
	indestructable = 1;
};

datablock ShapeBaseImageData(slyrCTF_FlagImage)
{
	shapeFile = "{{flag_shape}}";
	emap = true;
	mountPoint = {{flag_mount}};
	offset = "{{flag_offset}}";
	eyeOffset = "{{flag_eye_offset}}";
	rotation = eulerToMatrix("{{flag_rotation}}");
	armReady = false;
	doColorShift = true;
	colorShiftColor = "1 1 1 1";

	stateName[0] = "Idle";
	stateTransitionOnTimeout[0] = "Idle";
	stateTimeoutValue[0] = 2;
	stateSequence[0] = "root";
};

datablock ItemData(slyrCTF_FlagItem)
{
	shapeFile = "{{flag_shape}}";
	uiName = "{{flag_name}}";
	image = slyrCTF_FlagImage;
	doColorShift = true;
	colorShiftColor = "1 1 1 1";
	canDrop = false;
};
