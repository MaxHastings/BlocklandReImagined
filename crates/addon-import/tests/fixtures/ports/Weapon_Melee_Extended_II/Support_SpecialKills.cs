// Stand-in special kill messages, written for these tests.
function addSpecialDamageMsg(%name, %murderMsg, %suicideMsg)
{
   if($SpecialDamage_NumTypes $= "")
      $SpecialDamage_NumTypes = -1;
   if($SpecialDamage_TypeID[%name] $= "")
      %id = $SpecialDamage_NumTypes++;
   else
      %id = $SpecialDamage_TypeID[%name];
   $SpecialDamage_TypeID[%name] = %id;
   $SpecialDamage_TypeName[%id] = %name;
   $SpecialDamage_MurderMessage[%id] = %murderMsg;
   $SpecialDamage_SuicideMessage[%id] = %suicideMsg;
}

package StandinSpecialKills
{
   function GameConnection::onDeath(%this, %sourceObject, %sourceClient, %damageType, %damageArea)
   {
      %curmsg = "%2%3%1";
      for(%i = 0; $SpecialDamage_TypeName[%i] !$= ""; %i++)
      {
         if(!call("isSpecialKill_" @ $SpecialDamage_TypeName[%i], %this, %sourceObject, %sourceClient))
            continue;
         %msg = $SpecialDamage_MurderMessage[%i];
         %pos1 = strPos(%msg, "%2");
         %pos2 = strPos(%msg, "%1");
         %ciString = getSubStr(%msg, %pos1 + 2, %pos2 - %pos1 - 2);
         %curmsg = strReplace(%curmsg, "%3", %ciString);
         %special = 1;
      }
      if(%special)
      {
         %msg = getTaggedString($DeathMessage_Murder[%damageType]);
         %pos1 = strPos(%msg, "%2");
         %pos2 = strPos(%msg, "%1");
         %ciString = getSubStr(%msg, %pos1 + 2, %pos2 - %pos1 - 2);
         %msg = strReplace(%curmsg, "%3", %ciString);
         messageAll('MsgClientKilled', %msg);
      }
      Parent::onDeath(%this, %sourceObject, %sourceClient, %damageType, %damageArea);
   }
};
activatePackage(StandinSpecialKills);
