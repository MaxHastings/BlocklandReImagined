// Stand-in for the Weapon_Skins_Bullpup port tests (CC0): our own skins on the stand-in
// hosts, under the datablock and method names the port covers.
ForceRequiredAddOn("Weapon_Package_Tier2A");

datablock ProjectileData(CompactBullpupProjectile1 : BullpupProjectile1)
{
	directDamage = 9;
};

datablock ItemData(CompactBullpupItem : BullpupItem)
{
	uiName = "Stand-in CompactBullpup";
	image = CompactBullpupImage;
	TT_maxAmmo = 7;
};

datablock ShapeBaseImageData(CompactBullpupImage : BullpupImage)
{
	item = CompactBullpupItem;
	projectile = CompactBullpupProjectile1;
};

datablock ProjectileData(TacticalBullpupProjectile1 : BullpupProjectile1)
{
	directDamage = 12;
};

datablock ItemData(TacticalBullpupItem : BullpupItem)
{
	uiName = "Stand-in TacticalBullpup";
	image = TacticalBullpupImage;
	TT_maxAmmo = 10;
};

datablock ShapeBaseImageData(TacticalBullpupImage : BullpupImage)
{
	item = TacticalBullpupItem;
	projectile = TacticalBullpupProjectile1;
};

function CompactBullpupImage::onFire(%this,%obj,%slot)
{
	if(vectorLen(%obj.getVelocity()) < 0.1)
	{
	%spread = 0.0002;
	}
	else
	{
	%spread = 0.0008;
	%shellCount = 1;
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
	}
}

function CompactBullpupImage::onBurstFire(%this,%obj,%slot)
{
	if(vectorLen(%obj.getVelocity()) < 0.1)
	{
	%spread = 0.0002;
	}
	else
	{
	%spread = 0.0008;
	%shellCount = 1;
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
	}
}

function CompactBullpupImage::onBurstCheck(%this, %obj, %slot)
{
	if(%this.TT_canFire(%obj))
	{
	%obj.setImageLoaded(%slot, 1);
	%this.TT_decrementAmmo(%obj);
	}
}

function TacticalBullpupImage::onFire(%this,%obj,%slot)
{
	if(vectorLen(%obj.getVelocity()) < 0.1)
	{
	%spread = 0.0002;
	}
	else
	{
	%spread = 0.0008;
	%shellCount = 1;
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
	}
}

function TacticalBullpupImage::onBurstFire(%this,%obj,%slot)
{
	if(vectorLen(%obj.getVelocity()) < 0.1)
	{
	%spread = 0.0002;
	}
	else
	{
	%spread = 0.0008;
	%shellCount = 1;
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
	}
}

function TacticalBullpupImage::onBurstCheck(%this, %obj, %slot)
{
	if(%this.TT_canFire(%obj))
	{
	%obj.setImageLoaded(%slot, 1);
	%this.TT_decrementAmmo(%obj);
	}
}
