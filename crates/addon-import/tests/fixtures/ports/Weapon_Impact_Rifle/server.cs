// Stand-in for the Weapon_Impact_Rifle port tests (CC0): our own rifle on
// the stand-in Tier 1, under the datablock and method names the port covers.
ForceRequiredAddOn("Weapon_Package_Tier1");

datablock ProjectileData(ImpactRifleProjectile : standinRifleProjectile)
{
   directDamage = 41;
};

datablock ItemData(ImpactRifleItem : standinRifleItem)
{
   uiName = "Stand-in Impact";
   image = ImpactRifleImage;
   TT_maxAmmo = 3;
};

datablock ShapeBaseImageData(ImpactRifleImage : standinRifleImage)
{
   item = ImpactRifleItem;
   projectile = ImpactRifleProjectile;
};

function ImpactRifleImage::onFire(%this,%obj,%slot)
{
   %obj.playThread(2, shiftAway);

   %this.TT_decrementAmmo(%obj);
   %this.TT_displayAmmo(%obj);

   %projectile = %this.projectile;
   if(vectorLen(%obj.getVelocity()) < 0.5 && (getSimTime() - %obj.lastShotTime) > 800)
   {
      %spread = 0.0007;
   }
   else
   {
      %spread = 0.0003;
   }
   %shellCount = 1;

   return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function ImpactRifleImage::onReloaded(%this,%obj,%slot)
{
   %this.TT_reload(%obj, %slot, standinClickSound, plant);
}
