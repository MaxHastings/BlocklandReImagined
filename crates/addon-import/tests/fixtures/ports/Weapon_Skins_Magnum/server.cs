// Stand-in for the Weapon_Skins_Magnum port tests (CC0): our own skins on the stand-in
// hosts, under the datablock and method names the port covers.
ForceRequiredAddOn("Weapon_Package_Tier2");

datablock ProjectileData(SemiMagnumProjectile : MagnumProjectile)
{
	directDamage = 9;
};

datablock ItemData(SemiMagnumItem : MagnumItem)
{
	uiName = "Stand-in SemiMagnum";
	image = SemiMagnumImage;
	TT_maxAmmo = 7;
};

datablock ShapeBaseImageData(SemiMagnumImage : MagnumImage)
{
	item = SemiMagnumItem;
	projectile = SemiMagnumProjectile;
};

datablock ProjectileData(WesternMagnumProjectile : MagnumProjectile)
{
	directDamage = 12;
};

datablock ItemData(WesternMagnumItem : MagnumItem)
{
	uiName = "Stand-in WesternMagnum";
	image = WesternMagnumImage;
	TT_maxAmmo = 10;
};

datablock ShapeBaseImageData(WesternMagnumImage : MagnumImage)
{
	item = WesternMagnumItem;
	projectile = WesternMagnumProjectile;
};

datablock ProjectileData(MavMagnumProjectile : MagnumProjectile)
{
	directDamage = 15;
};

datablock ItemData(MavMagnumItem : MagnumItem)
{
	uiName = "Stand-in MavMagnum";
	image = MavMagnumImage;
	TT_maxAmmo = 6;
};

datablock ShapeBaseImageData(MavMagnumImage : MagnumImage)
{
	item = MavMagnumItem;
	projectile = MavMagnumProjectile;
};

datablock ProjectileData(RetroMagnumProjectile : MagnumProjectile)
{
	directDamage = 9;
};

datablock ItemData(RetroMagnumItem : MagnumItem)
{
	uiName = "Stand-in RetroMagnum";
	image = RetroMagnumImage;
	TT_maxAmmo = 9;
};

datablock ShapeBaseImageData(RetroMagnumImage : MagnumImage)
{
	item = RetroMagnumItem;
	projectile = RetroMagnumProjectile;
};

function semiMagnumImage::onFire(%this,%obj,%slot)
{
	%spread = 0.0001;
	%shellCount = 1;
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function semiMagnumProjectile::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	TT_processHeadshotDamage(%this, %obj, %col, %pos, %this.directDamage, 2, $DamageType::MagnumHeadshot);
}

function WesternMagnumImage::onFire(%this,%obj,%slot)
{
	%spread = 0.0001;
	%shellCount = 1;
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function WesternMagnumImage::onReloaded(%this,%obj,%slot)
{
	%this.TT_incrementReload(%obj, %slot);
}

function WesternMagnumProjectile::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	TT_processHeadshotDamage(%this, %obj, %col, %pos, %this.directDamage, 2, $DamageType::MagnumHeadshot);
}

function MavMagnumImage::onFire(%this,%obj,%slot)
{
	%spread = 0.0001;
	%shellCount = 1;
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function MavMagnumImage::onReloaded(%this,%obj,%slot)
{
	%this.TT_incrementReload(%obj, %slot);
}

function MavMagnumProjectile::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	TT_processHeadshotDamage(%this, %obj, %col, %pos, %this.directDamage, 2, $DamageType::MagnumHeadshot);
}

function RetroMagnumImage::onFire(%this,%obj,%slot)
{
	%spread = 0.0001;
	%shellCount = 1;
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function RetroMagnumProjectile::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	TT_processHeadshotDamage(%this, %obj, %col, %pos, %this.directDamage, 2, $DamageType::MagnumHeadshot);
}
