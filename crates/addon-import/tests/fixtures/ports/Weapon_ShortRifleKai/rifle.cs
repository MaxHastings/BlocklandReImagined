AddDamageType("StandinShort", '%1 bounced one into themselves', '%2 shot %1', 0.75, 1);
AddDamageType("StandinCrit", '%1 bounced one into themselves', '%2 banked one off into %1', 0.75, 1);

datablock ProjectileData(shortRifleProjectile)
{
   directDamage        = 0;
   directDamageType    = $DamageType::StandinShort;
   radiusDamageType    = $DamageType::StandinShort;
};

datablock ItemData(shortRifleItem : standinSidearmItem)
{
   shapeFile = "./short.dts";
   uiName = "Stand-in Short Rifle";
   image = shortRifleImage;
   TT_ammoType = "556";
   TT_maxAmmo = 5;
};

datablock ShapeBaseImageData(shortRifleImage : standinSidearmImage)
{
   shapeFile = "./short.dts";
   item = shortRifleItem;
   projectile = shortRifleProjectile;
   TT_raycastEnabled = false;
   TT_raycastTracerProjectile = "";

   raycastWeaponRange = 80;
   raycastWeaponTargets = $TypeMasks::PlayerObjectType | $TypeMasks::FXBrickObjectType;
   raycastExplosionProjectile = shortRifleProjectile;
   raycastExplosionBrickSound = standinClickSound;
   raycastExplosionPlayerSound = standinClickSound;
   raycastDirectDamage = 16;
   raycastDirectDamageType = $DamageType::StandinShort;
   raycastCritDirectDamageType = $DamageType::StandinCrit;
   raycastFromMuzzle = true;
   raycastRicochets = 2;

   stateScript[10]                  = "onReload";
};

function shortRifleImage::onReload(%this, %obj, %slot)
{
   %this.TT_reload(%obj, %slot);
   %this.TT_displayAmmo(%obj);
}

function shortRifleImage::onFire(%this, %obj, %slot)
{
   %this.TT_decrementAmmo(%obj);
   %this.TT_displayAmmo(%obj);
   %obj.playThread(2, shiftAway);
   %this.fireRaycast(%obj, %slot);
}
