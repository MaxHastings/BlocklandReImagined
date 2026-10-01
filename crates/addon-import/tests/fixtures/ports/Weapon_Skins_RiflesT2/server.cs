// Stand-in for the Weapon_Skins_RiflesT2 port tests (CC0): our own skins on the stand-in
// hosts, under the datablock and method names the port covers.
ForceRequiredAddOn("Weapon_Package_Tier2");

datablock ProjectileData(ModernTAssaultRifleProjectile1 : TAssaultRifleProjectile1)
{
	directDamage = 9;
};

datablock ProjectileData(ModernTAssaultRifleProjectile2 : TAssaultRifleProjectile2)
{
	directDamage = 10;
};

datablock ItemData(ModernTAssaultRifleItem : TAssaultRifleItem)
{
	uiName = "Stand-in ModernTAssaultRifle";
	image = ModernTAssaultRifleImage;
	TT_maxAmmo = 8;
};

datablock ShapeBaseImageData(ModernTAssaultRifleImage : TAssaultRifleImage)
{
	item = ModernTAssaultRifleItem;
	projectile = ModernTAssaultRifleProjectile1;
};

datablock ProjectileData(ClassicTAssaultRifleProjectile1 : TAssaultRifleProjectile1)
{
	directDamage = 13;
};

datablock ProjectileData(ClassicTAssaultRifleProjectile2 : TAssaultRifleProjectile2)
{
	directDamage = 14;
};

datablock ItemData(ClassicTAssaultRifleItem : TAssaultRifleItem)
{
	uiName = "Stand-in ClassicTAssaultRifle";
	image = ClassicTAssaultRifleImage;
	TT_maxAmmo = 5;
};

datablock ShapeBaseImageData(ClassicTAssaultRifleImage : TAssaultRifleImage)
{
	item = ClassicTAssaultRifleItem;
	projectile = ClassicTAssaultRifleProjectile1;
};

datablock ProjectileData(ClassicBattleRifleProjectile1 : BattleRifleProjectile1)
{
	directDamage = 8;
};

datablock ItemData(ClassicBattleRifleItem : BattleRifleItem)
{
	uiName = "Stand-in ClassicBattleRifle";
	image = ClassicBattleRifleImage;
	TT_maxAmmo = 8;
};

datablock ShapeBaseImageData(ClassicBattleRifleImage : BattleRifleImage)
{
	item = ClassicBattleRifleItem;
	projectile = ClassicBattleRifleProjectile1;
};

datablock ProjectileData(BrowningTAssaultRifleProjectile1 : TAssaultRifleProjectile1)
{
	directDamage = 11;
};

datablock ProjectileData(BrowningTAssaultRifleProjectile2 : TAssaultRifleProjectile2)
{
	directDamage = 12;
};

datablock ItemData(BrowningTAssaultRifleItem : TAssaultRifleItem)
{
	uiName = "Stand-in BrowningTAssaultRifle";
	image = BrowningTAssaultRifleImage;
	TT_maxAmmo = 5;
};

datablock ShapeBaseImageData(BrowningTAssaultRifleImage : TAssaultRifleImage)
{
	item = BrowningTAssaultRifleItem;
	projectile = BrowningTAssaultRifleProjectile1;
};

datablock ProjectileData(ScoutTAssaultRifleProjectile1 : TAssaultRifleProjectile1)
{
	directDamage = 15;
};

datablock ProjectileData(ScoutTAssaultRifleProjectile2 : TAssaultRifleProjectile2)
{
	directDamage = 16;
};

datablock ItemData(ScoutTAssaultRifleItem : TAssaultRifleItem)
{
	uiName = "Stand-in ScoutTAssaultRifle";
	image = ScoutTAssaultRifleImage;
	TT_maxAmmo = 9;
};

datablock ShapeBaseImageData(ScoutTAssaultRifleImage : TAssaultRifleImage)
{
	item = ScoutTAssaultRifleItem;
	projectile = ScoutTAssaultRifleProjectile1;
};

datablock ProjectileData(SemiBattleRifleProjectile1 : BattleRifleProjectile1)
{
	directDamage = 10;
};

datablock ItemData(SemiBattleRifleItem : BattleRifleItem)
{
	uiName = "Stand-in SemiBattleRifle";
	image = SemiBattleRifleImage;
	TT_maxAmmo = 5;
};

datablock ShapeBaseImageData(SemiBattleRifleImage : BattleRifleImage)
{
	item = SemiBattleRifleItem;
	projectile = SemiBattleRifleProjectile1;
};

datablock ProjectileData(CompactTAssaultRifleProjectile1 : TAssaultRifleProjectile1)
{
	directDamage = 13;
};

datablock ProjectileData(CompactTAssaultRifleProjectile2 : TAssaultRifleProjectile2)
{
	directDamage = 14;
};

datablock ItemData(CompactTAssaultRifleItem : TAssaultRifleItem)
{
	uiName = "Stand-in CompactTAssaultRifle";
	image = CompactTAssaultRifleImage;
	TT_maxAmmo = 9;
};

datablock ShapeBaseImageData(CompactTAssaultRifleImage : TAssaultRifleImage)
{
	item = CompactTAssaultRifleItem;
	projectile = CompactTAssaultRifleProjectile1;
};

function ModernTAssaultRifleImage::onFire(%this,%obj,%slot)
{
   if((getSimTime() - %obj.lastShotTime) > 400)
   {
      %projectile = ModernTAssaultRifleProjectile2;
      %spread = 0.0002;
   }
   else
   {
      %projectile = %this.projectile;
      %spread = 0.003;
   }
   %shellCount = 1;
   %obj.lastShotTime = getSimTime();
   %this.TT_decrementAmmo(%obj);
   return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function ClassicTAssaultRifleImage::onFire(%this,%obj,%slot)
{
   if((getSimTime() - %obj.lastShotTime) > 400)
   {
      %projectile = ModernTAssaultRifleProjectile2;
      %spread = 0.0002;
   }
   else
   {
      %projectile = %this.projectile;
      %spread = 0.003;
   }
   %shellCount = 1;
   %obj.lastShotTime = getSimTime();
   %this.TT_decrementAmmo(%obj);
   return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function ClassicBattleRifleImage::onFire(%this,%obj,%slot)
{
   TT_knockback(%obj, 0, 0, -2);
   %projectile = %this.projectile;
   %spread = 0.0005;
   %shellCount = 1;
   %this.TT_decrementAmmo(%obj);
   return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function BattlerifleProjectile1::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	Parent::damage(%this,%obj,%col,%fade,%pos,%normal);
	TT_dampenVelocity(%col, 2);
}

function BrowningTAssaultRifleImage::onFire(%this,%obj,%slot)
{
   if((getSimTime() - %obj.lastShotTime) > 400)
   {
      %projectile = ModernTAssaultRifleProjectile2;
      %spread = 0.0002;
   }
   else
   {
      %projectile = %this.projectile;
      %spread = 0.003;
   }
   %shellCount = 1;
   %obj.lastShotTime = getSimTime();
   %this.TT_decrementAmmo(%obj);
   return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function ScoutTAssaultRifleImage::onFire(%this,%obj,%slot)
{
   if((getSimTime() - %obj.lastShotTime) > 400)
   {
      %projectile = ModernTAssaultRifleProjectile2;
      %spread = 0.0002;
   }
   else
   {
      %projectile = %this.projectile;
      %spread = 0.003;
   }
   %shellCount = 1;
   %obj.lastShotTime = getSimTime();
   %this.TT_decrementAmmo(%obj);
   return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function SemiBattleRifleImage::onFire(%this,%obj,%slot)
{
   TT_knockback(%obj, 0, 0, -2);
   %projectile = %this.projectile;
   %spread = 0.0005;
   %shellCount = 1;
   %this.TT_decrementAmmo(%obj);
   return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function CompactTAssaultRifleImage::onFire(%this,%obj,%slot)
{
   if((getSimTime() - %obj.lastShotTime) > 400)
   {
      %projectile = ModernTAssaultRifleProjectile2;
      %spread = 0.0002;
   }
   else
   {
      %projectile = %this.projectile;
      %spread = 0.003;
   }
   %shellCount = 1;
   %obj.lastShotTime = getSimTime();
   %this.TT_decrementAmmo(%obj);
   return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}
