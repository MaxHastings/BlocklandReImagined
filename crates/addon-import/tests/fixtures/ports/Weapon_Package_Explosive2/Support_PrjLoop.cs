// A projectile that runs a method of its datablock every so often, and
// throws projectiles out from one.
package StandinPrjLoop
{
   function Projectile::onAdd(%obj)
   {
	if(%db.PrjLoop_enabled)
	%obj.PrjLoop_Tick = %obj.schedule(%db.PrjLoop_tickTime, PrjLoop_tick);
}
};
activatePackage(StandinPrjLoop);

function Projectile::PrjLoop_tick(%prj)
{
	if(%prj.ticks >= %db.PrjLoop_maxTicks && %db.PrjLoop_maxTicks != -1)
	return;
}

function ProjectileData::PrjLoop_onTick(%db, %prj)
{
}

function PrjLoop_emitPrj(%origPrj, %emittedPrj, %speed, %amount)
{
	%vec = mcos(%a) SPC msin(%a) SPC mcos(%b);
}
