// Stand-in medical pack: a needle gun and a syringe that mend people.
function TT_defaultIfUnset(%pref, %default, %category)
{
   if(%category $= "")
      %category = "TT";
   if($Pref::Server["::" @ %category @ "::" @ %pref] $= "")
      $Pref::Server["::" @ %category @ "::" @ %pref] = %default;
}

// Its two preferences, in RTB's server control when it is there.
if($RTB::Hooks::ServerControl)
{
   if(isFunction(registerPreferenceAddon))
      %mod = "Weapon_Package_Tier1";
   else
      %mod = "Weapon_Package_Medic1";
   RTB_registerPref("Heal Other Teams","Stand-in | Medical","$Pref::Server::TT::MedicHealEnemy","bool",%mod,0,0,1);
   RTB_registerPref("Heal Bots","Stand-in | Medical","$Pref::Server::TT::MedicHealBots","bool",%mod,0,0,1);
}
else
{
   TT_defaultIfUnset("MedicHealEnemy", 0);
   TT_defaultIfUnset("MedicHealBots", 0);
}

if(ForceRequiredAddOn("Weapon_Gun") == $Error::AddOn_NotFound)
{
   error("Weapon_Package_Medic1 needs Weapon_Gun, which is missing.");
}
else
{
   exec("./Support_ImageAltTrigger.cs");
   exec("./Item_GauzeGun.cs");
   exec("./Item_Stimpack.cs");
   exec("./Emote_Heal.cs");
}

// Who may mend whom.
function TT_canHeal(%client, %targetObject)
{
	if(%client && (%targetObject.getType() & $TypeMasks::PlayerObjectType) && ($Pref::Server::TT::MedicHealBots || %targetObject.getClassName() $= "Player"))
	{
	if(%mini == getMinigameFromObject(%targetObject))
	{
	if(!$Pref::Server::TT::MedicHealEnemy && %mini.isSlayerMinigame)
	{
	else if($Server::LAN && !isObject(getMinigameFromObject(%targetObject)))
	return 1;
	if(%targetObject.spawnBrick.getGroup().bl_id == getBL_IDFromObject(%client))
	return 1;
	}}}
}

// A mending hit: some health now, a little more over time.
function TT_projectileHeal(%obj, %col, %healBurst)
{
	if(TT_canHeal(%healer, %col))
	{
	if(%col.getDamageLevel() >= %healBurst)
	%col.spawnExplosion(healCrossProjectile, %col.getScale());
	%col.setDamageLevel(%col.getDamageLevel() - %healBurst);
	if(%colName !$= "")
	bottomPrint(%obj.client, "\c2" @ %colName @ " is patched up.", 2, 1);
	if(%healerName !$= "")
	bottomPrint(%col.client, "\c2" @ %healerName @ " patched you up.", 2, 1);
	%col.emote(medigunHealImage);
	}
}
