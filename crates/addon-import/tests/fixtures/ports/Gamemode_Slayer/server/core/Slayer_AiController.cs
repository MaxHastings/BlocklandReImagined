// Stand-in (CC0): the skin colours the port reads. Not the original's
// code or values.
$Slayer::Server::Bots::SkinColor[1] = "0.9 0.8 0.6 1";
$Slayer::Server::Bots::SkinColor[2] = "0.4 0.3 0.2 1";
$Slayer::Server::Bots::SkinColor[3] = "0.2 0.1 0.05 1";
$Slayer::Server::Bots::SkinColor[4] = "0.7 0.5 0.4 1";
$Slayer::Server::Bots::SkinColorCount = 4;

// Stand-ins (CC0) for the bot shapes the port reads.
function Slayer_AiController::assignObjectives(%this)
{
	
}

function Slayer_AiController::createPlayer(%this, %transform)
{
	%hBotType = BlockheadHoleBot;
	hReturnToSpawn = !%useFakeBrick;
	%i ++)
	%this.forceEquip(%i, %handler.startEquip[%i]);
	%this.applyUniform();
	%player.setShapeNameColor(%handler.colorRGB);
}

function Slayer_AiController::onSpawn(%this)
{
	%this.player.useRandomTool();
}

function Slayer_AiController::onDeath(%this, %obj, %killer, %type, %area)
{
	if(!%this.dead())
	{
	%spawnTime = %mini.botRespawnTime;
	}
}

function Slayer_AiController::applyBodyColors(%this)
{
	%color = $Slayer::Server::Bots::SkinColor[getRandom($Slayer::Server::Bots::SkinColorCount)];
	%this.player.setNodeColor(headSkin, %color);
}
