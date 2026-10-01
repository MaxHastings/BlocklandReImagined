// Stand-in for the Weapon_Skins_SMG port tests (CC0): our own skins on the stand-in
// hosts, under the datablock and method names the port covers.
ForceRequiredAddOn("Weapon_Package_Tier1");

datablock ProjectileData(NavalSubmachinegunProjectile1 : standinSMGProjectile)
{
	directDamage = 9;
};

datablock ItemData(NavalSMGItem : standinSMGItem)
{
	uiName = "Stand-in NavalSMG";
	image = NavalSMGImage;
	TT_maxAmmo = 7;
};

datablock ShapeBaseImageData(NavalSMGImage : standinSMGImage)
{
	item = NavalSMGItem;
	projectile = NavalSubmachinegunProjectile1;
};

datablock ProjectileData(ClassicSubmachinegunProjectile1 : standinSMGProjectile)
{
	directDamage = 12;
};

datablock ItemData(ClassicSMGItem : standinSMGItem)
{
	uiName = "Stand-in ClassicSMG";
	image = ClassicSMGImage;
	TT_maxAmmo = 10;
};

datablock ShapeBaseImageData(ClassicSMGImage : standinSMGImage)
{
	item = ClassicSMGItem;
	projectile = ClassicSubmachinegunProjectile1;
};

datablock ProjectileData(ModernSubmachinegunProjectile1 : standinSMGProjectile)
{
	directDamage = 15;
};

datablock ItemData(ModernSMGItem : standinSMGItem)
{
	uiName = "Stand-in ModernSMG";
	image = ModernSMGImage;
	TT_maxAmmo = 6;
};

datablock ShapeBaseImageData(ModernSMGImage : standinSMGImage)
{
	item = ModernSMGItem;
	projectile = ModernSubmachinegunProjectile1;
};

datablock ProjectileData(SilencedSubmachinegunProjectile1 : standinSMGProjectile)
{
	directDamage = 9;
};

datablock ItemData(SilencedSMGItem : standinSMGItem)
{
	uiName = "Stand-in SilencedSMG";
	image = SilencedSMGImage;
	TT_maxAmmo = 9;
};

datablock ShapeBaseImageData(SilencedSMGImage : standinSMGImage)
{
	item = SilencedSMGItem;
	projectile = SilencedSubmachinegunProjectile1;
};

datablock ProjectileData(MicroSubmachinegunProjectile1 : standinSMGProjectile)
{
	directDamage = 12;
};

datablock ItemData(MicroSMGItem : standinSMGItem)
{
	uiName = "Stand-in MicroSMG";
	image = MicroSMGImage;
	TT_maxAmmo = 5;
};

datablock ShapeBaseImageData(MicroSMGImage : standinSMGImage)
{
	item = MicroSMGItem;
	projectile = MicroSubmachinegunProjectile1;
};

function NavalSMGImage::onFire(%this,%obj,%slot)
{
	%spread = 0.002;
	%shellCount = 1;
	%obj.playThread(2, plant);
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function NavalSubmachinegunProjectile1::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	TT_dampenVelocity(%col, 2);
}

function ClassicSMGImage::onFire(%this,%obj,%slot)
{
	%spread = 0.002;
	%shellCount = 1;
	%obj.playThread(2, plant);
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function ClassicSubmachinegunProjectile1::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	TT_dampenVelocity(%col, 2);
}

function ModernSMGImage::onFire(%this,%obj,%slot)
{
	%spread = 0.002;
	%shellCount = 1;
	%obj.playThread(2, plant);
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function ModernSubmachinegunProjectile1::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	TT_dampenVelocity(%col, 2);
}

function SilencedSMGImage::onFire(%this,%obj,%slot)
{
	%spread = 0.002;
	%shellCount = 1;
	%obj.playThread(2, plant);
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function SilencedSubmachinegunProjectile1::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	TT_dampenVelocity(%col, 2);
}

function MicroSMGImage::onFire(%this,%obj,%slot)
{
	%spread = 0.002;
	%shellCount = 1;
	%obj.playThread(2, plant);
	%this.TT_decrementAmmo(%obj);
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function MicroSubmachinegunProjectile1::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	TT_dampenVelocity(%col, 2);
}
