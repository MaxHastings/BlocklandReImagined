// Stand-in capture point functions (CC0): each in the shape the port's
// patterns read. Not the original's code.
function FxDtsBrick::createTrigger(%this, %data, %polyhedron)
{
	%boxDiff = vectorSub(%boxMax, %boxMin);
	%boxDiff = vectorAdd(%boxDiff, "0 0 0.2");
	return %trigger;
}

function FxDtsBrick::setCPControl(%this, %color, %reset, %client)
{
	if(!%reset)
		%client.incScore(%mini.points_cp);
	%this.setColor(%color);
	if(%reset)
		%this.processInputEvent("onCPReset", %client);
	else
	{
		%this.processInputEvent("onCPCapture", %client);
		%this.processInputEvent("onCPCapture(Team" @ %i + 1 @ ")", %client);
	}
}

function Slayer_CPTriggerData::onTickTrigger(%this, %trigger, %player)
{
	if(%brick.capture[%attCol] < %maxTicks)
	{
		%trigger.decreaseTimer[%attCol] = %this.scheduleNoQuota(%this.tickPeriodMS + 1000, decreaseCapture, %trigger, %attCol);
		%brick.capture[%attCol] ++;
		%client.bottomPrint("<just:center>" @ %a @ %d, 1, true);
		if(%mini.CPTransitionColors)
		{
			%mix = Slayer_Support::getAverageColor(%rgb, %team.colorRGB);
			%brick.setColor(Slayer_Support::getClosestPaintColor(%mix));
		}
	}
	else
		%brick.setCPControl(%attCol, 0, %client);
}

function Slayer_CPTriggerData::decreaseCapture(%this, %trigger, %color)
{
	if(%cl.slyrTeam.color == %color) return;
	%brick.capture[%color] --;
	if(%brick.capture[%color] <= 0) %brick.setColor(%defCol);
	else %trigger.decreaseTimer[%color] = %this.scheduleNoQuota(%this.tickPeriodMS, decreaseCapture, %trigger, %color);
}
