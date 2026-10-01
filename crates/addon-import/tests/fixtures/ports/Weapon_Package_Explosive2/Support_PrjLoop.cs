// A projectile that runs a method of its datablock every so often, and
// throws projectiles out from one.
package StandinPrjLoop
{
   function Projectile::onAdd(%obj)
   {
      %db = %obj.getDatablock();
      if(%db.PrjLoop_enabled)
         %obj.PrjLoop_Tick = %obj.schedule(%db.PrjLoop_tickTime, PrjLoop_tick);
      Parent::onAdd(%obj);
   }
};
activatePackage(StandinPrjLoop);

function Projectile::PrjLoop_tick(%prj)
{
   %db = %prj.getDatablock();
   %db.PrjLoop_onTick(%prj);
   %prj.ticks++;
   if(%prj.ticks >= %db.PrjLoop_maxTicks && %db.PrjLoop_maxTicks != -1)
      return;
   %prj.PrjLoop_Tick = %prj.schedule(%db.PrjLoop_tickTime, PrjLoop_tick);
}

function ProjectileData::PrjLoop_onTick(%db, %prj)
{
}

function PrjLoop_emitPrj(%origPrj, %emittedPrj, %speed, %amount)
{
   for(%i = 0; %i < %amount; %i++)
   {
      %a = getRandom(0, 360) / 360 * 2 * $pi;
      %b = getRandom(0, 360) / 360 * 2 * $pi;
      %vec = mcos(%a) SPC msin(%a) SPC mcos(%b);
      %p = new Projectile()
      {
         dataBlock = %emittedPrj;
         initialVelocity = vectorScale(%vec, %speed);
         initialPosition = %origPrj.getPosition();
         sourceObject = %origPrj.sourceObject;
         client = %origPrj.client;
      };
   }
}
