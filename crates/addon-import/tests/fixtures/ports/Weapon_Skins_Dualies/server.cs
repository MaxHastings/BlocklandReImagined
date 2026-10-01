// Stand-in for the Weapon_Skins_Dualies port tests (CC0): our own skins on the stand-in
// hosts, under the datablock and method names the port covers.
ForceRequiredAddOn("Weapon_Skins_Pistol");

datablock ItemData(AkimboClassicPistolItem : ClassicPistolItem)
{
	uiName = "Stand-in AkimboClassicPistol";
	image = AkimboClassicPistolImage;
	TT_maxAmmo = classicpistolitem.TT_maxAmmo * 2;
};

datablock ShapeBaseImageData(AkimboClassicPistolImage : standinPairImage)
{
	item = AkimboClassicPistolItem;
	TT_raycastDirectDamage = classicpistolimage.TT_raycastDirectDamage;
};

datablock ShapeBaseImageData(LeftHandedClassicPistolImage : standinLeftImage)
{
	item = AkimboClassicPistolItem;
	projectile = standinPelletProjectile;
	TT_raycastDirectDamage = classicpistolimage.TT_raycastDirectDamage;
};

datablock ItemData(AkimboModernPistolItem : ModernPistolItem)
{
	uiName = "Stand-in AkimboModernPistol";
	image = AkimboModernPistolImage;
	TT_maxAmmo = Modernpistolitem.TT_maxAmmo * 2;
};

datablock ShapeBaseImageData(AkimboModernPistolImage : standinPairImage)
{
	item = AkimboModernPistolItem;
	TT_raycastDirectDamage = Modernpistolimage.TT_raycastDirectDamage;
};

datablock ShapeBaseImageData(LeftHandedModernPistolImage : standinLeftImage)
{
	item = AkimboModernPistolItem;
	projectile = standinPelletProjectile;
	TT_raycastDirectDamage = Modernpistolimage.TT_raycastDirectDamage;
};

datablock ItemData(AkimboSilencedPistolItem : SilencedPistolItem)
{
	uiName = "Stand-in AkimboSilencedPistol";
	image = AkimboSilencedPistolImage;
	TT_maxAmmo = Silencedpistolitem.TT_maxAmmo * 2;
};

datablock ShapeBaseImageData(AkimboSilencedPistolImage : standinPairImage)
{
	item = AkimboSilencedPistolItem;
	TT_raycastDirectDamage = Silencedpistolimage.TT_raycastDirectDamage;
};

datablock ShapeBaseImageData(LeftHandedSilencedPistolImage : standinLeftImage)
{
	item = AkimboSilencedPistolItem;
	projectile = standinPelletProjectile;
	TT_raycastDirectDamage = Silencedpistolimage.TT_raycastDirectDamage;
};

function AkimboClassicPistolImage::onMount(%this, %obj, %slot)
{
	%obj.mountImage(LeftHandedClassicPistolImage, 1);
}

function AkimboClassicPistolImage::onUnMount(%this,%obj,%slot)
{
	%obj.unMountImage(1);
}

function AkimboClassicPistolImage::onFire(%this,%obj,%slot)
{
	Parent::onFire(%this,%obj,%slot);
	%this.TT_decrementAmmo(%obj);
}

function AkimboClassicPistolImage::onFireAkimbo(%this,%obj,%slot)
{
	%obj.setImageTrigger(1,1);
}

function LeftHandedClassicPistolImage::onFire(%this,%obj,%slot)
{
	if(vectorLen(%obj.getVelocity()) > 0.1)
	{
	%this.TT_raycastSpreadAmt = 0.003;
	%this.TT_raycastWeaponRange = 50;
	}
	else
	{
	%this.TT_raycastSpreadAmt = 0.001;
	%this.TT_raycastWeaponRange = 120;
	%this.TT_decrementAmmo(%obj);
	return Parent::onFire(%this,%obj,%slot);
	}
}

function AkimboModernPistolImage::onMount(%this, %obj, %slot)
{
	%obj.mountImage(LeftHandedModernPistolImage, 1);
}

function AkimboModernPistolImage::onUnMount(%this,%obj,%slot)
{
	%obj.unMountImage(1);
}

function AkimboModernPistolImage::onFire(%this,%obj,%slot)
{
	Parent::onFire(%this,%obj,%slot);
	%this.TT_decrementAmmo(%obj);
}

function AkimboModernPistolImage::onFireAkimbo(%this,%obj,%slot)
{
	%obj.setImageTrigger(1,1);
}

function LeftHandedModernPistolImage::onFire(%this,%obj,%slot)
{
	if(vectorLen(%obj.getVelocity()) > 0.1)
	{
	%this.TT_raycastSpreadAmt = 0.003;
	%this.TT_raycastWeaponRange = 50;
	}
	else
	{
	%this.TT_raycastSpreadAmt = 0.001;
	%this.TT_raycastWeaponRange = 120;
	%this.TT_decrementAmmo(%obj);
	return Parent::onFire(%this,%obj,%slot);
	}
}

function AkimboSilencedPistolImage::onMount(%this, %obj, %slot)
{
	%obj.mountImage(LeftHandedSilencedPistolImage, 1);
}

function AkimboSilencedPistolImage::onUnMount(%this,%obj,%slot)
{
	%obj.unMountImage(1);
}

function AkimboSilencedPistolImage::onFire(%this,%obj,%slot)
{
	Parent::onFire(%this,%obj,%slot);
	%this.TT_decrementAmmo(%obj);
}

function AkimboSilencedPistolImage::onFireAkimbo(%this,%obj,%slot)
{
	%obj.setImageTrigger(1,1);
}

function LeftHandedSilencedPistolImage::onFire(%this,%obj,%slot)
{
	if(vectorLen(%obj.getVelocity()) > 0.1)
	{
	%this.TT_raycastSpreadAmt = 0.003;
	%this.TT_raycastWeaponRange = 50;
	}
	else
	{
	%this.TT_raycastSpreadAmt = 0.001;
	%this.TT_raycastWeaponRange = 120;
	%this.TT_decrementAmmo(%obj);
	return Parent::onFire(%this,%obj,%slot);
	}
}
