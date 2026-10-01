// Stand-in medical pack: a needle gun and a syringe that mend people.
function TT_defaultIfUnset(%pref, %default, %category)
{
   if(%category $= "")
      %category = "TT";
   if($Pref::Server["::" @ %category @ "::" @ %pref] $= "")
      $Pref::Server["::" @ %category @ "::" @ %pref] = %default;
}

TT_defaultIfUnset("MedicHealEnemy", 0);
TT_defaultIfUnset("MedicHealBots", 0);

if(ForceRequiredAddOn("Weapon_Gun") == $Error::AddOn_NotFound)
{
   error("ERROR: Weapon_Package_Medic1 - required add-on Weapon_Gun not found");
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
      if(isObject(%mini = getMinigameFromObject(%client)))
      {
         if(%mini == getMinigameFromObject(%targetObject))
         {
            if(!$Pref::Server::TT::MedicHealEnemy && %mini.isSlayerMinigame)
            {
               %team1 = %client.getTeam();
               %team2 = %targetObject.client.getTeam();
               if(!%team1 || %team1.isAlliedTeam(%team2))
                  return 1;
            }
            else
               return 1;
         }
      }
      else if($Server::LAN && !isObject(getMinigameFromObject(%targetObject)))
         return 1;
      else if(isObject(%targetObject.spawnBrick) && %targetObject.getClassName() $= "AIPlayer")
      {
         if(%targetObject.spawnBrick.getGroup().bl_id == getBL_IDFromObject(%client))
            return 1;
      }
   }
   return 0;
}

// A mending hit: some health now, a little more over time.
function TT_projectileHeal(%obj, %col, %healBurst)
{
   %healer = %obj.client;
   if(TT_canHeal(%healer, %col))
   {
      if(%col.getDamageLevel() >= %healBurst)
         %col.spawnExplosion(healCrossProjectile, %col.getScale());
      %col.setDamageLevel(%col.getDamageLevel() - %healBurst);

      %colName = %col.getPlayerName();
      %healerName = %healer.getPlayerName();
      if(%colName !$= "")
         bottomPrint(%obj.client, "\c2" @ %colName @ " is patched up.", 2, 1);
      if(%healerName !$= "")
         bottomPrint(%col.client, "\c2" @ %healerName @ " patched you up.", 2, 1);
      %col.emote(medigunHealImage);
   }
}
