// Stand-in for the Weapon_Skins_Sniper port tests (CC0): our own skins on the stand-in
// hosts, under the datablock and method names the port covers.
ForceRequiredAddOn("Weapon_Package_Tier2");

datablock ProjectileData(magmilTracerProjectile : standinTracerProjectile)
{
	directDamage = 9;
};

datablock ItemData(MagnifiedSniperRifleItem : MilitarySniperItem)
{
	uiName = "Stand-in MagnifiedSniperRifle";
	image = MagnifiedSniperRifleImage;
	TT_maxAmmo = 7;
};

datablock ShapeBaseImageData(MagnifiedSniperRifleImage : MilitarySniperImage)
{
	item = MagnifiedSniperRifleItem;
	projectile = magmilTracerProjectile;
	TT_raycastDirectDamage = 13;
};

datablock ProjectileData(clasmilTracerProjectile : standinTracerProjectile)
{
	directDamage = 12;
};

datablock ItemData(ClassicSniperRifleItem : MilitarySniperItem)
{
	uiName = "Stand-in ClassicSniperRifle";
	image = ClassicSniperRifleImage;
	TT_maxAmmo = 10;
};

datablock ShapeBaseImageData(ClassicSniperRifleImage : MilitarySniperImage)
{
	item = ClassicSniperRifleItem;
	projectile = clasmilTracerProjectile;
	TT_raycastDirectDamage = 16;
};

datablock ProjectileData(semimilTracerProjectile : standinTracerProjectile)
{
	directDamage = 15;
};

datablock ItemData(SemiSniperRifleItem : MilitarySniperItem)
{
	uiName = "Stand-in SemiSniperRifle";
	image = SemiSniperRifleImage;
	TT_maxAmmo = 6;
};

datablock ShapeBaseImageData(SemiSniperRifleImage : MilitarySniperImage)
{
	item = SemiSniperRifleItem;
	projectile = semimilTracerProjectile;
	TT_raycastDirectDamage = 10;
};

function MagnifiedSniperRifleImage::onFire(%this,%obj,%slot)
{
   %this.TT_decrementAmmo(%obj);
   return Parent::onFire(%this,%obj,%slot);
}

function MagnifiedSniperRifleImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
   return TT_isRaycastHeadshot(%this, %obj, %slot, %col, %pos, %normal, %hit);
}

function ClassicSniperRifleImage::onFire(%this,%obj,%slot)
{
   %this.TT_decrementAmmo(%obj);
   return Parent::onFire(%this,%obj,%slot);
}

function ClassicSniperRifleImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
   return TT_isRaycastHeadshot(%this, %obj, %slot, %col, %pos, %normal, %hit);
}

function SemiSniperRifleImage::onFire(%this,%obj,%slot)
{
   %this.TT_decrementAmmo(%obj);
   return Parent::onFire(%this,%obj,%slot);
}

function SemiSniperRifleImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
   return TT_isRaycastHeadshot(%this, %obj, %slot, %col, %pos, %normal, %hit);
}
