// Stand-in for the Weapon_Package_Tier1A port tests (CC0): our own guns on
// the stand-in Tier 1, under the method names the port covers.
ForceRequiredAddOn("Weapon_Package_Tier1");

AddDamageType("StandinSnub", '%1 shot themselves', '%2 shot %1', 0.75, 1);
AddDamageType("SnubnoseHeadshot", '%1 shot themselves', '%2 headshot %1', 0.75, 1);

// The single shotgun: pellets, a close blast and a shove back.
datablock ProjectileData(SingleShotgunBlastProjectile : standinBlastProjectile)
{
   directDamage = 2;
};

datablock ItemData(SingleShotgunItem : standinSidearmItem)
{
   shapeFile = "./single.dts";
   uiName = "Stand-in Single";
   image = SingleShotgunImage;
   TT_ammoType = "shotgun";
   TT_maxAmmo = 2;
};

datablock ShapeBaseImageData(SingleShotgunImage : standinSidearmImage)
{
   item = SingleShotgunItem;
   projectile = standinPelletProjectile;
   TT_raycastEnabled = false;
};

function SingleShotgunImage::onFire(%this,%obj,%slot)
{
	TT_knockback(%obj, -3, -3, -3);
	%this.TT_decrementAmmo(%obj);
	if($Pref::Server::TT::Recoil)
	%obj.spawnExplosion(standinKickProjectile,"1 1 1");
	TT_createProjectile(%this, %obj, %slot, SingleShotgunBlastProjectile, 1);
	%projectile = %this.projectile;
	%spread = 0.002;
	%shellCount = 6;
	return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

// The pepperbox: three rays a shot.
datablock ItemData(PepperPistolItem : standinSidearmItem)
{
   uiName = "Stand-in Pepper";
   image = PepperPistolImage;
   TT_maxAmmo = 3;
};

datablock ShapeBaseImageData(PepperPistolImage : standinSidearmImage)
{
   item = PepperPistolItem;
   TT_raycastSpreadAmt = 0.004;
   TT_raycastSpreadCount = 3;
   TT_raycastDirectDamage = 5;
};

function PepperpistolImage::onFire(%this,%obj,%slot)
{
	Parent::onFire(%this,%obj,%slot);
	%this.TT_decrementAmmo(%obj);
}

// The snubnose: twice its damage to the head.
datablock ProjectileData(SnubnoseProjectile : standinRifleProjectile)
{
   directDamage = 15;
   directDamageType = $DamageType::StandinSnub;
};

datablock ItemData(SnubnoseItem : standinSidearmItem)
{
   uiName = "Stand-in Snub";
   image = SnubnoseImage;
   TT_ammoType = "556";
   TT_maxAmmo = 5;
};

datablock ShapeBaseImageData(SnubnoseImage : standinSidearmImage)
{
   item = SnubnoseItem;
   projectile = SnubnoseProjectile;
   TT_raycastEnabled = false;
};

function SnubnoseProjectile::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	TT_processHeadshotDamage(%this, %obj, %col, %pos, %this.directDamage, 2, $DamageType::SnubnoseHeadshot);
}

// The easter egg, loaded only with the hidden setting.
datablock ProjectileData(nailgunProjectile1 : standinSMGProjectile)
{
   directDamage = 2;
};

datablock ItemData(nailgunItem : standinSidearmItem)
{
   uiName = "Stand-in Nails";
   image = nailgunImage;
   TT_maxAmmo = 8;
};

datablock ShapeBaseImageData(nailgunImage : standinSidearmImage)
{
   item = nailgunItem;
   projectile = nailgunProjectile1;
   TT_raycastEnabled = false;
};

function nailgunProjectile1::damage(%this,%obj,%col,%fade,%pos,%normal)
{
	if(%col.getType() & $TypeMasks::PlayerObjectType)
	TT_dampenVelocity(%col, 1.5);
}
