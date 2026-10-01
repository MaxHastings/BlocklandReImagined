// Stand-in game modes (CC0): the original's layout, this stand-in's names.
new ScriptGroup(Slayer_DefaultGameModeTemplateSG)
{
	class = "Slayer_GameModeTemplateSG";
	className = "Slayer_Deathmatch";
	uiName = "Free for All";
	useTeams = false;
};

new ScriptGroup(Slayer_GameModeTemplateSG)
{
	className = "Slayer_TeamDeathmatch";
	uiName = "Teams";
	useTeams = true;
};
