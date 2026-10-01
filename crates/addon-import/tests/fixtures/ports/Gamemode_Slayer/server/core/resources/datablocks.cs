// Stand-in (CC0). Its parent is base-stand-ins.cs's stand-in for the base
// game's brickSpawnPointData.
datablock FxDtsBrickData(brickSlyrSpawnPointData : brickSpawnPointData)
{
	uiName = "Stand-in Team Spawn";
};

// Stand-in capture points on a stand-in pad, with this stand-in's own bar
// lengths; their trigger reads the tick time preference.
datablock fxDTSBrickData(brickStandInPadData)
{
	brickFile = "./pad.blb";
	uiName = "Stand-in Pad";
};

datablock TriggerData(Slayer_CPTriggerData)
{
	tickPeriodMS = $Pref::Slayer::Server::CPTriggerTickMS;
};

datablock FxDtsBrickData(brickSlyrCPData : brickStandInPadData)
{
	uiName = "Stand-in Capture Point";
	isSlyrBrick = true;
	slyrType = "CP";
	CPMaxTicks = 3;
};

datablock FxDtsBrickData(brickSlyrLrgCPData : brickStandInPadData)
{
	uiName = "Stand-in Large Capture Point";
	isSlyrBrick = true;
	slyrType = "CP";
	CPMaxTicks = 5;
};

datablock PathCameraData(Slayer_SpectatePathCamData : Observer)
{
	maxNodes = 6;
};
