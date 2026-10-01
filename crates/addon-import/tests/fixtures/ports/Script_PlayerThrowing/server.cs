// Stand-in for the Script_PlayerThrowing port tests (CC0): a small
// pick-up-and-throw script with the function names and shapes the port
// reads, and its own numbers (held at 0.75 scale, reach 3, throws 1 to 30,
// a 10-notch charge, a 2.5 front check, watched from 6 units out (4 to 9),
// turned a quarter right on the hand).
$Pref::Server::PlayerThrowing::GrabTimeout = 5;
$Pref::Server::PlayerThrowing::EscapeTimeout = 3;
$Pref::Server::PlayerThrowing::ChargeRate = 100;
$Pref::Server::PlayerThrowing::Multiplier = 2.5;
$Pref::Server::PlayerThrowing::ThrowAmount = 12;

function Player::isHoldingPlayer(%this)
{
	return isObject(%this.holdingPlayer);
}

function Player::isBeingHeld(%this)
{
	return isObject(%this.holderPlayer);
}

function Player::isFrontClear(%this, %mask, %multiplier)
{
	if(%multiplier $= "")
	%multiplier = 2.5;
}

function Player::canGrab(%this, %object)
{
	if(!isObject(%targetClient) || %targetClient.miniGame != %client.miniGame)
	return false;
	if(miniGameCanDamage(%this, %object) != 1)
	return false;
	return !%object.isBeingHeld();
}

function Player::attemptGrab(%this)
{
	if(getSimTime() - %this.grabTimeout < 5000)
	return false;
	%end = vectorAdd(%this.getEyePoint(), vectorScale(%vector, 3 * %zScale));
	%col = firstWord(containerRaycast(%this.getEyePoint(), %end, $TypeMasks::PlayerObjectType, %this));
	if(%col.isMounted())
	%col.dismount();
	%col.unmountImage(0);
	%this.mountObject(%col, 0);
	%col.setScale(vectorScale(%this.getScale(), 0.75));
	%col.playThread(1, death1);
	%col.setTransform(%col.getPosition() @ " 0 0 1 1.5708");
	%col.setLookLimits(0.6, 0.4);
	%col.client.camera.setOrbitMode(%this, 0, 4, 9, 6, 0);
	%this.playThread(0, armReadyBoth);
}

function Player::removeFromGrasp(%this)
{
	%this.setLookLimits(1, 0);
	%throw = %holder.isFrontClear();
	if(getWord(%holder.getEyeVector(), 2) <= -0.85 && getWord(%holder.getPosition(), 2) < 0.3)
	%throw = false;
}

function Player::clearAllHolding(%this)
{
	if(%this.isBeingHeld())
	%this.removeFromGrasp();
}

function Player::escapeFromGrasp(%this)
{
	if(getSimTime() - %this.grabbedAt < $Pref::Server::PlayerThrowing::EscapeTimeout * 1000)
	return false;
}

function Player::throwPlayer(%this, %amount)
{
	%amount = mClamp(%amount, 1, 30);
	if(%held.removeFromGrasp())
	%held.addVelocity(vectorScale(%eye, %amount * $Pref::Server::PlayerThrowing::Multiplier));
}

function Player::chargeThrow(%this)
{
	%amount = mClamp(%this.chargeThrowAmount, 1, 10);
	%this.chargeThrowAmount = %amount + 1;
}

function PlayerThrowing_CanUseTools(%client)
{
	return !(%player.isBeingHeld() || %player.isHoldingPlayer());
}

package Script_PlayerThrowing
{
   function Armor::onTrigger(%this, %obj, %triggerNum, %val)
   {
	if(%triggerNum == 0 && %obj.isHoldingPlayer())
	{
	}
}

   function Player::activateStuff(%this)
   {
	if(%this.isBeingHeld() && %this.escapeFromGrasp())
	return;
	if(%this.isHoldingPlayer() || %this.attemptGrab())
	return;
}

   function Observer::onTrigger(%this, %obj, %trigger, %state)
   {
	if(%obj.mode !$= "Grabbed")
	return Parent::onTrigger(%this, %obj, %trigger, %state);
	if(%trigger == 0 && %state)
	%player.escapeFromGrasp();
}

   function Armor::onRemove(%this, %obj)
   {
	%obj.clearAllHolding();
}

   function GameConnection::onDeath(%this, %killerPlayer, %killerClient, %damageType, %damageLoc)
   {
	%this.player.clearAllHolding();
}

   function serverCmdUseInventory(%client, %slot)
   {
	if(PlayerThrowing_CanUseTools(%client))
	Parent::serverCmdUseInventory(%client, %slot);
}

   function serverCmdInstantUseBrick(%client, %data)
   {
	if(PlayerThrowing_CanUseTools(%client))
	Parent::serverCmdInstantUseBrick(%client, %data);
}

   function serverCmdUseTool(%client, %slot)
   {
	if(PlayerThrowing_CanUseTools(%client))
	Parent::serverCmdUseTool(%client, %slot);
}

   function serverCmdUnuseTool(%client)
   {
	if(PlayerThrowing_CanUseTools(%client))
	Parent::serverCmdUnuseTool(%client);
}

   function serverCmdUseSprayCan(%client, %slot)
   {
	if(PlayerThrowing_CanUseTools(%client))
	Parent::serverCmdUseSprayCan(%client, %slot);
}

   function serverCmdUseFxCan(%client, %slot)
   {
	if(PlayerThrowing_CanUseTools(%client))
	Parent::serverCmdUseFxCan(%client, %slot);
}
};
activatePackage(Script_PlayerThrowing);
