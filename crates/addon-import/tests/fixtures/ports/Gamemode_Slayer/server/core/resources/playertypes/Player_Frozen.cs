// Stand-in (CC0) with the original's shape: a player type that cannot move
// or use items, worn through the pre-round countdown.
datablock PlayerData(PlayerFrozenArmor : PlayerStandardArmor)
{
	thirdPersonOnly = 1;
	runForce = 0;
	maxForwardSpeed = 0;
	maxBackwardSpeed = 0;
	maxSideSpeed = 0;
	maxForwardCrouchSpeed = 0;
	maxBackwardCrouchSpeed = 0;
	maxSideCrouchSpeed = 0;
	jumpForce = 0;
	jumpEnergyDrain = 0;
	minJumpEnergy = 0;
	canJet = 0;
	runSurfaceAngle = 0;
	jumpSurfaceAngle = 0;
	uiName = "";
};

function PlayerFrozenArmor::onTrigger(%this, %player, %slot, %state)
{
	return;
}

package Stand_In_Frozen
{
	function serverCmdUseTool(%client, %slot)
	{
		if(%client.player.getDatablock().getID() != PlayerFrozenArmor.getID())
			parent::serverCmdUseTool(%client, %slot);
	}
};
activatePackage(Stand_In_Frozen);
