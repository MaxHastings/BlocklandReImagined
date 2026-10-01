// Stand-in for the Weapon_Skins_ShotgunT2 port tests (CC0): our own skins on the stand-in
// hosts, under the datablock and method names the port covers.
ForceRequiredAddOn("Weapon_Package_Tier2");

datablock ProjectileData(SlickShotgunProjectile : CombatShotgunProjectile)
{
	directDamage = 9;
};

datablock ProjectileData(SlickShotgunBlastProjectile : standinBlastProjectile)
{
	directDamage = 10;
};

datablock ItemData(SlickShotgunItem : CombatShotgunItem)
{
	uiName = "Stand-in SlickShotgun";
	image = SlickShotgunImage;
	TT_maxAmmo = 8;
};

datablock ShapeBaseImageData(SlickShotgunImage : CombatShotgunImage)
{
	item = SlickShotgunItem;
	projectile = SlickShotgunProjectile;
};

datablock ProjectileData(TacticalShotgunProjectile : CombatShotgunProjectile)
{
	directDamage = 13;
};

datablock ProjectileData(TacticalShotgunBlastProjectile : standinBlastProjectile)
{
	directDamage = 14;
};

datablock ItemData(tacticalShotgunItem : CombatShotgunItem)
{
	uiName = "Stand-in tacticalShotgun";
	image = tacticalShotgunImage;
	TT_maxAmmo = 5;
};

datablock ShapeBaseImageData(tacticalShotgunImage : CombatShotgunImage)
{
	item = tacticalShotgunItem;
	projectile = tacticalShotgunProjectile;
};

function SlickShotgunImage::onFire(%this,%obj,%slot)
{
	%this.TT_decrementAmmo(%obj);
	%projectile = %this.projectile;
	%spread = 0.004;
	%shellCount = 5;
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function SlickShotgunImage::onReloaded(%this,%obj,%slot)
{
	%this.TT_incrementReload(%obj, %slot);
}

function tacticalShotgunImage::onFire(%this,%obj,%slot)
{
	%this.TT_decrementAmmo(%obj);
	%projectile = %this.projectile;
	%spread = 0.004;
	%shellCount = 5;
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function tacticalShotgunImage::onReloaded(%this,%obj,%slot)
{
	%this.TT_incrementReload(%obj, %slot);
}
