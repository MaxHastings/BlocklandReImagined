// Stand-in for the Weapon_Skins_Shotgun port tests (CC0): our own skins on the stand-in
// hosts, under the datablock and method names the port covers.
ForceRequiredAddOn("Weapon_Package_Tier1");

datablock ProjectileData(SquaredshotgunProjectile : standinPelletProjectile)
{
	directDamage = 9;
};

datablock ProjectileData(SquaredshotgunBlastProjectile : standinBlastProjectile)
{
	directDamage = 10;
};

datablock ItemData(SquaredShotgunItem : standinPumpItem)
{
	uiName = "Stand-in SquaredShotgun";
	image = SquaredShotgunImage;
	TT_maxAmmo = 8;
};

datablock ShapeBaseImageData(SquaredShotgunImage : standinPumpImage)
{
	item = SquaredShotgunItem;
	projectile = SquaredshotgunProjectile;
};

datablock ProjectileData(ScattergunProjectile : standinPelletProjectile)
{
	directDamage = 13;
};

datablock ProjectileData(ScattergunBlastProjectile : standinBlastProjectile)
{
	directDamage = 14;
};

datablock ItemData(ScattergunItem : standinPumpItem)
{
	uiName = "Stand-in Scattergun";
	image = ScattergunImage;
	TT_maxAmmo = 5;
};

datablock ShapeBaseImageData(ScattergunImage : standinPumpImage)
{
	item = ScattergunItem;
	projectile = ScattergunProjectile;
};

datablock ProjectileData(ClassicShotgunProjectile : standinPelletProjectile)
{
	directDamage = 8;
};

datablock ProjectileData(ClassicShotgunBlastProjectile : standinBlastProjectile)
{
	directDamage = 9;
};

datablock ItemData(ClassicShotgunItem : standinPumpItem)
{
	uiName = "Stand-in ClassicShotgun";
	image = ClassicShotgunImage;
	TT_maxAmmo = 9;
};

datablock ShapeBaseImageData(ClassicShotgunImage : standinPumpImage)
{
	item = ClassicShotgunItem;
	projectile = ClassicShotgunProjectile;
};

function SquaredShotgunImage::onFire(%this,%obj,%slot)
{
   if(%this.TT_canFire(%obj))
   {
      serverPlay3D(standinBoomSound,%obj.getPosition());
      %obj.playThread(2, activate);

      %this.TT_decrementAmmo(%obj);

      if($Pref::Server::TT::Recoil)
         %obj.spawnExplosion(standinKickProjectile,"1 1 1");

      %projectile = %this.projectile;
      %spread = 0.004;
      %shellCount = 5;

      %p = TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
      TT_createProjectile(%this, %obj, %slot, SquaredshotgunBlastProjectile, 1);
   }
   else if(!$Pref::Server::TT::DeathStopFiring || %obj.getDamagePercent() < 1.0)
   {
      serverPlay3D(standinJamSound,%obj.getPosition());
   }
   %this.TT_displayAmmo(%obj);
   return %p;
}

function SquaredShotgunImage::onReloaded(%this,%obj,%slot)
{
   %this.TT_incrementReload(%obj, %slot);
   %this.TT_displayAmmo(%obj);
}

function ScattergunImage::onFire(%this,%obj,%slot)
{
   if(%this.TT_canFire(%obj))
   {
      serverPlay3D(standinBoomSound,%obj.getPosition());
      %obj.playThread(2, activate);

      %this.TT_decrementAmmo(%obj);

      if($Pref::Server::TT::Recoil)
         %obj.spawnExplosion(standinKickProjectile,"1 1 1");

      %projectile = %this.projectile;
      %spread = 0.004;
      %shellCount = 5;

      %p = TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
      TT_createProjectile(%this, %obj, %slot, SquaredshotgunBlastProjectile, 1);
   }
   else if(!$Pref::Server::TT::DeathStopFiring || %obj.getDamagePercent() < 1.0)
   {
      serverPlay3D(standinJamSound,%obj.getPosition());
   }
   %this.TT_displayAmmo(%obj);
   return %p;
}

function ScattergunImage::onReloaded(%this,%obj,%slot)
{
   %this.TT_incrementReload(%obj, %slot);
   %this.TT_displayAmmo(%obj);
}

function classicShotgunImage::onFire(%this,%obj,%slot)
{
   if(%this.TT_canFire(%obj))
   {
      serverPlay3D(standinBoomSound,%obj.getPosition());
      %obj.playThread(2, activate);

      %this.TT_decrementAmmo(%obj);

      if($Pref::Server::TT::Recoil)
         %obj.spawnExplosion(standinKickProjectile,"1 1 1");

      %projectile = %this.projectile;
      %spread = 0.004;
      %shellCount = 5;

      %p = TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
      TT_createProjectile(%this, %obj, %slot, SquaredshotgunBlastProjectile, 1);
   }
   else if(!$Pref::Server::TT::DeathStopFiring || %obj.getDamagePercent() < 1.0)
   {
      serverPlay3D(standinJamSound,%obj.getPosition());
   }
   %this.TT_displayAmmo(%obj);
   return %p;
}

function ClassicShotgunImage::onReloaded(%this,%obj,%slot)
{
   %this.TT_incrementReload(%obj, %slot);
   %this.TT_displayAmmo(%obj);
}
