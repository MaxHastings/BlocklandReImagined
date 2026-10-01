// CC0 stand-in: the wrench output that hands a player rounds.
function initAddAmmoTT()
{
	%choices = "list ALL 0";
	for(%n = 0; %n < TT_allAmmoTypesGroup.getCount(); %n++)
		%choices = %choices SPC TT_allAmmoTypesGroup.getObject(%n).name SPC %n + 1;
	%params = %choices TAB "int -1 9999 -1" TAB "bool";
	registerOutputEvent(Player, "AddAmmoTT", %params);
	registerOutputEvent(Bot, "AddAmmoTT", %params);
}
initAddAmmoTT();

function Player::addAmmoTT(%pl, %listIdx, %amount, %ignoreMax)
{
	if(%listIdx == 0)
	{
		for(%n = TT_allAmmoTypesGroup.getCount() - 1; %n >= 0; %n--)
			TT_addAmmo(%pl, TT_allAmmoTypesGroup.getObject(%n).name, %amount, %ignoreMax);
		return;
	}
	TT_addAmmo(%pl, TT_allAmmoTypesGroup.getObject(%listIdx - 1).name, %amount, %ignoreMax);
}
