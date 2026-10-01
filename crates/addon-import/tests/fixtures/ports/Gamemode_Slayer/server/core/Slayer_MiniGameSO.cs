// Stand-in (CC0): the spawn choice's shape the port reads.
function Slayer_MiniGameSO::pickSpawnPoint(%this, %client)
{
	%team = %client.getTeam();
	for(%i = 0; %i < %numSpawns; %i ++)
	{
		%sp = Slayer.Spawns.getObject(%i);
		%type = %sp.getDatablock().slyrType;
		%color = %sp.getTeamControl();
		if(%type !$= "TeamSpawn" || %color != %team.color)
			continue;
		if(!%sp.isBlocked())
			%open ++;
	}
	if(%db.getName() !$= "brickSpawnPointData")
		return;
}
