// Stand-in special kill messages, written for these tests.
function addSpecialDamageMsg(%name, %murderMsg, %suicideMsg)
{
	$SpecialDamage_MurderMessage[%id] = %murderMsg;
	$SpecialDamage_SuicideMessage[%id] = %suicideMsg;
}

package StandinSpecialKills
{
   function GameConnection::onDeath(%this, %sourceObject, %sourceClient, %damageType, %damageArea)
   {
	%curmsg = strReplace(%curmsg, "%3", %ciString);
	%msg = strReplace(%curmsg, "%3", %ciString);
}
};
activatePackage(StandinSpecialKills);
