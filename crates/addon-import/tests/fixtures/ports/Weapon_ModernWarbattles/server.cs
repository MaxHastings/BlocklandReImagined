// Stand-in for the Weapon_ModernWarbattles port tests (CC0): the hl2 ammo
// system's shape, with our own guns, names and numbers.
AddDamageType("StandinPistol", '%1 shot themselves', '%2 shot %1', 0.75, 1);
AddDamageType("StandinShotgun", '%1 shot themselves', '%2 shot %1', 0.75, 1);

datablock ProjectileData(standinPistolProjectile)
{
   directDamage        = 10;
   directDamageType    = $DamageType::StandinPistol;
   muzzleVelocity      = 90;
   velInheritFactor    = 1;
   lifetime            = 2000;
   isBallistic         = false;
   gravityMod          = 0.0;
   headshotMultiplier  = 1.5;
};

datablock ProjectileData(standinShotgunProjectile)
{
   directDamage        = 8;
   directDamageType    = $DamageType::StandinShotgun;
   muzzleVelocity      = 90;
   velInheritFactor    = 1;
   lifetime            = 2000;
   isBallistic         = false;
   gravityMod          = 0.0;
   headshotMultiplier  = 1;
};

datablock ItemData(standinPistolItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./pistol.dts";
   uiName = "Stand-in Pistol";
   image = standinPistolImage;
   canDrop = true;
   maxmag = 12;
   ammotype = "Pistol";
   reload = true;
   nochamber = 1;
};

datablock ShapeBaseImageData(standinPistolImage)
{
   shapeFile = "./pistol.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = standinPistolItem;
   projectile = standinPistolProjectile;
   projectileType = Projectile;
   armReady = true;

   stateName[0]                     = "Activate";
   stateTimeoutValue[0]             = 0.1;
   stateTransitionOnTimeout[0]      = "LoadCheckA";

   stateName[1]                     = "Ready";
   stateTransitionOnTriggerDown[1]  = "Fire";
   stateTransitionOnNoAmmo[1]       = "ReloadStart";
   stateAllowImageChange[1]         = true;

   stateName[2]                     = "Fire";
   stateTransitionOnTimeout[2]      = "LoadCheckA";
   stateTimeoutValue[2]             = 0.15;
   stateFire[2]                     = true;
   stateScript[2]                   = "onFire";
   stateWaitForTimeout[2]           = true;

   stateName[3]                     = "LoadCheckA";
   stateScript[3]                   = "onLoadCheck";
   stateTimeoutValue[3]             = 0.01;
   stateTransitionOnTimeout[3]      = "LoadCheckB";

   stateName[4]                     = "LoadCheckB";
   stateTransitionOnAmmo[4]         = "Ready";
   stateTransitionOnNoAmmo[4]       = "ReloadStart";

   stateName[5]                     = "ReloadStart";
   stateScript[5]                   = "onReloadStart";
   stateTimeoutValue[5]             = 0.5;
   stateTransitionOnTimeout[5]      = "Reload";
   stateWaitForTimeout[5]           = true;

   stateName[6]                     = "Reload";
   stateScript[6]                   = "onReload";
   stateTimeoutValue[6]             = 1.0;
   stateTransitionOnTimeout[6]      = "LoadCheckA";
   stateWaitForTimeout[6]           = true;
};

datablock ItemData(huntingShotgunItem : standinPistolItem)
{
   shapeFile = "./shotgun.dts";
   uiName = "Stand-in Shotgun";
   image = standinShotgunImage;
   maxmag = 5;
   ammotype = "Shotgun";
};

datablock ShapeBaseImageData(standinShotgunImage : standinPistolImage)
{
   shapeFile = "./shotgun.dts";
   item = huntingShotgunItem;
   projectile = standinShotgunProjectile;
   stateTimeoutValue[6]             = 0.4;
};

datablock ItemData(standinAmmoItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./box.dts";
   uiName = "Ammo [ALL]";
   image = standinPistolImage;
   canDrop = true;
   ammoBox = true;
};

datablock ItemData(standinAmmoPistolItem : standinAmmoItem)
{
   uiName = "Ammo [Pistol]";
   ammotype = "Pistol";
};

function hl2AmmoOnReload(%this, %obj, %slot)
{
   %item = %this.item;
   %need = %item.maxmag - %obj.toolMag[%obj.currTool];
   %have = %obj.toolAmmo[%item.ammotype];
   %move = %need < %have ? %need : %have;
   %obj.toolMag[%obj.currTool] += %move;
   %obj.toolAmmo[%item.ammotype] -= %move;
}

function standinPistolProjectile::damage(%this, %obj, %col, %fade, %pos, %normal)
{
   %damage = %this.directDamage;
   if(%col.isCrouched() || getHitbox(%obj, %col, %pos) $= "headSkin")
      %damage *= %this.headshotMultiplier;
   %col.damage(%obj, %pos, %damage, %this.directDamageType);
}

package standinAmmoSystem
{
   function Armor::onCollision(%this, %obj, %col, %vec, %speed)
   {
      if(%col.getDataBlock().ammoBox)
      {
         %obj.toolAmmo[%col.getDataBlock().ammotype] += 4;
         %obj.toolAmmo[%col.getDataBlock().ammotype] += 4;
         return;
      }
      Parent::onCollision(%this, %obj, %col, %vec, %speed);
   }
};
activatePackage(standinAmmoSystem);
