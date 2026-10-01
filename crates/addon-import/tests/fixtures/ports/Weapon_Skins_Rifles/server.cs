// Stand-in for the Weapon_Skins_Rifles port tests (CC0): our own skins on the stand-in
// hosts, under the datablock and method names the port covers.
ForceRequiredAddOn("Weapon_Package_Tier1");

datablock ProjectileData(BoltRifleProjectile : standinRifleProjectile)
{
	directDamage = 9;
};

datablock ProjectileData(BoltRifleWeakProjectile : standinRifleWeakProjectile)
{
	directDamage = 10;
};

datablock ItemData(BoltRifleItem : standinRifleItem)
{
	uiName = "Stand-in BoltRifle";
	image = BoltRifleImage;
	TT_maxAmmo = 8;
};

datablock ShapeBaseImageData(BoltRifleImage : standinRifleImage)
{
	item = BoltRifleItem;
	projectile = BoltRifleProjectile;
};

datablock ProjectileData(HuntingRifleProjectile : standinRifleProjectile)
{
	directDamage = 13;
};

datablock ProjectileData(HuntingRifleWeakProjectile : standinRifleWeakProjectile)
{
	directDamage = 14;
};

datablock ItemData(HuntingRifleItem : standinRifleItem)
{
	uiName = "Stand-in HuntingRifle";
	image = HuntingRifleImage;
	TT_maxAmmo = 5;
};

datablock ShapeBaseImageData(HuntingRifleImage : standinRifleImage)
{
	item = HuntingRifleItem;
	projectile = HuntingRifleProjectile;
};

datablock ProjectileData(RetroRifleProjectile : standinRifleProjectile)
{
	directDamage = 8;
};

datablock ProjectileData(RetroRifleWeakProjectile : standinRifleWeakProjectile)
{
	directDamage = 9;
};

datablock ItemData(RetroRifleItem : standinRifleItem)
{
	uiName = "Stand-in RetroRifle";
	image = RetroRifleImage;
	TT_maxAmmo = 9;
};

datablock ShapeBaseImageData(RetroRifleImage : standinRifleImage)
{
	item = RetroRifleItem;
	projectile = RetroRifleProjectile;
};

function BoltRifleImage::onFire(%this,%obj,%slot)
{
	%obj.playThread(2, plant);
	if(vectorLen(%obj.getVelocity()) < 3 && (getSimTime() - %obj.lastShotTime) > 1000)
	{
	%projectile = %this.projectile;
	%spread = 0.0001;
	}
	else
	{
	%projectile = BoltRifleWeakProjectile;
	%spread = 0.001;
	%shellCount = 1;
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
	}
}

function BoltRifleImage::onMount(%this,%obj,%slot)
{
	%obj.playThread(2, shiftLeft);
}

function BoltRifleProjectile::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	%multiplier = 2.5;
	TT_processHeadshotDamage(%this, %obj, %col, %pos, %this.directDamage, %multiplier, $DamageType::StandinRifleHeadshot);
}

function BoltRifleWeakProjectile::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	TT_processHeadshotDamage(%this, %obj, %col, %pos, %this.directDamage, 2, $DamageType::StandinRifleHeadshot);
}

function HuntingRifleImage::onFire(%this,%obj,%slot)
{
	%obj.playThread(2, plant);
	if(vectorLen(%obj.getVelocity()) < 3 && (getSimTime() - %obj.lastShotTime) > 1000)
	{
	%projectile = %this.projectile;
	%spread = 0.0001;
	}
	else
	{
	%projectile = BoltRifleWeakProjectile;
	%spread = 0.001;
	%shellCount = 1;
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
	}
}

function HuntingRifleProjectile::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	%multiplier = 2.5;
	TT_processHeadshotDamage(%this, %obj, %col, %pos, %this.directDamage, %multiplier, $DamageType::StandinRifleHeadshot);
}

function HuntingRifleWeakProjectile::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	TT_processHeadshotDamage(%this, %obj, %col, %pos, %this.directDamage, 2, $DamageType::StandinRifleHeadshot);
}

function RetroRifleImage::onFire(%this,%obj,%slot)
{
	%obj.playThread(2, plant);
	if(vectorLen(%obj.getVelocity()) < 3 && (getSimTime() - %obj.lastShotTime) > 1000)
	{
	%projectile = %this.projectile;
	%spread = 0.0001;
	}
	else
	{
	%projectile = BoltRifleWeakProjectile;
	%spread = 0.001;
	%shellCount = 1;
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
	}
}

function RetroRifleProjectile::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	%multiplier = 2.5;
	TT_processHeadshotDamage(%this, %obj, %col, %pos, %this.directDamage, %multiplier, $DamageType::StandinRifleHeadshot);
}

function RetroRifleWeakProjectile::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	TT_processHeadshotDamage(%this, %obj, %col, %pos, %this.directDamage, 2, $DamageType::StandinRifleHeadshot);
}
