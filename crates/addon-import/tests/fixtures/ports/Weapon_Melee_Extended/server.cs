// Stand-in for Tier+Tactical Melee Extended: our own datablocks, numbers
// and sounds, laid out as the original is so its port applies.
datablock AudioProfile(TT_MeleeSwingSound)
{
   filename = "./swing.wav";
   description = AudioClosest3d;
   preload = true;
};
datablock AudioProfile(standinThudSound : TT_MeleeSwingSound) { filename = "./thud.wav"; };
datablock AudioProfile(standinClinkSound : TT_MeleeSwingSound) { filename = "./clink.wav"; };
datablock AudioProfile(standinSliceSound : TT_MeleeSwingSound) { filename = "./slice.wav"; };

exec("./Support_TT_Raycasting.cs");
exec("./Weapons.cs");

function TT_isMeleeRaycastCrit(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
	return isObject(%col) && %col.iszombie;
}
