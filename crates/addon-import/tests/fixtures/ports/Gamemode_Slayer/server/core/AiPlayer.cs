// Stand-ins (CC0) for the bot shapes the port reads. Not the original's
// code.
function AiPlayer::useRandomTool(%this)
{
	%i ++)
	if(isObject(%this.tool[%i]) && (getRandom(0, 1) || %i == %indexEnd - 1))
	return %this.setWeapon(%this.tool[%i]);
}

package Slayer_AiPlayer
{
	function checkHoleBotTeams(%obj, %target, %neutralAttack, %melee)
	{
	if(%teamA.isAlliedTeam(%teamB))
	return 0;
}
};
activatePackage(Slayer_AiPlayer);
