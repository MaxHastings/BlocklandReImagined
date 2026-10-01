// Slayer CTF makes these at run time in createSlayerCTFDatablocks(): the
// two bricks, and one flag item and image of the host's Flag Type for each
// of the first ten paint colours. Here every flag type in the copy's table
// has its own. Here they are declared as the importer reads them, with one flag
// item and image that take their brick's or carrier's team colour (the
// engine tints each object, and the carried flag's light), so every paint
// colour has its flag. Values in
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

	hasLight = {{flag_has_light}};
	lightType = "{{flag_light_type}}";
	lightColor = "1 1 1 1";
	lightTime = {{flag_light_time}};
	lightRadius = {{flag_light_radius}};

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

// The copy's flag type 1 (Flag Type), as type 0 without its light or wave.
datablock ShapeBaseImageData(slyrCTF_FlagImage_1)
{
	shapeFile = "{{flag1_shape}}";
	emap = true;
	mountPoint = {{flag1_mount}};
	offset = "{{flag1_offset}}";
	eyeOffset = "{{flag_eye_offset}}";
	rotation = eulerToMatrix("{{flag1_rotation}}");
	armReady = false;
	doColorShift = true;
	colorShiftColor = "1 1 1 1";
};

datablock ItemData(slyrCTF_FlagItem_1)
{
	shapeFile = "{{flag1_shape}}";
	uiName = "{{flag1_name}}";
	image = slyrCTF_FlagImage_1;
	doColorShift = true;
	colorShiftColor = "1 1 1 1";
	canDrop = false;
};

// The copy's flag type 2 (Flag Type), as type 0 without its light or wave.
datablock ShapeBaseImageData(slyrCTF_FlagImage_2)
{
	shapeFile = "{{flag2_shape}}";
	emap = true;
	mountPoint = {{flag2_mount}};
	offset = "{{flag2_offset}}";
	eyeOffset = "{{flag_eye_offset}}";
	rotation = eulerToMatrix("{{flag2_rotation}}");
	armReady = false;
	doColorShift = true;
	colorShiftColor = "1 1 1 1";
};

datablock ItemData(slyrCTF_FlagItem_2)
{
	shapeFile = "{{flag2_shape}}";
	uiName = "{{flag2_name}}";
	image = slyrCTF_FlagImage_2;
	doColorShift = true;
	colorShiftColor = "1 1 1 1";
	canDrop = false;
};

// The copy's flag type 3 (Flag Type), as type 0 without its light or wave.
datablock ShapeBaseImageData(slyrCTF_FlagImage_3)
{
	shapeFile = "{{flag3_shape}}";
	emap = true;
	mountPoint = {{flag3_mount}};
	offset = "{{flag3_offset}}";
	eyeOffset = "{{flag_eye_offset}}";
	rotation = eulerToMatrix("{{flag3_rotation}}");
	armReady = false;
	doColorShift = true;
	colorShiftColor = "1 1 1 1";
};

datablock ItemData(slyrCTF_FlagItem_3)
{
	shapeFile = "{{flag3_shape}}";
	uiName = "{{flag3_name}}";
	image = slyrCTF_FlagImage_3;
	doColorShift = true;
	colorShiftColor = "1 1 1 1";
	canDrop = false;
};

// The copy's flag type 4 (Flag Type), as type 0 without its light or wave.
datablock ShapeBaseImageData(slyrCTF_FlagImage_4)
{
	shapeFile = "{{flag4_shape}}";
	emap = true;
	mountPoint = {{flag4_mount}};
	offset = "{{flag4_offset}}";
	eyeOffset = "{{flag_eye_offset}}";
	rotation = eulerToMatrix("{{flag4_rotation}}");
	armReady = false;
	doColorShift = true;
	colorShiftColor = "1 1 1 1";
};

datablock ItemData(slyrCTF_FlagItem_4)
{
	shapeFile = "{{flag4_shape}}";
	uiName = "{{flag4_name}}";
	image = slyrCTF_FlagImage_4;
	doColorShift = true;
	colorShiftColor = "1 1 1 1";
	canDrop = false;
};
