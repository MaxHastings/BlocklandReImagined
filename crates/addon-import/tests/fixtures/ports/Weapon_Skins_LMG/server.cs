// Stand-in for the Weapon_Skins_LMG port tests (CC0): our own skins on the stand-in
// hosts, under the datablock and method names the port covers.
ForceRequiredAddOn("Weapon_Package_Tier2");

datablock ProjectileData(ClassicLMGProjectile1 : LightMachinegunProjectile1)
{
	directDamage = 9;
};

datablock ItemData(ClassicLMGItem : LightMachinegunItem)
{
	uiName = "Stand-in ClassicLMG";
	image = ClassicLMGImage;
	TT_maxAmmo = 7;
};

datablock ShapeBaseImageData(ClassicLMGImage : LightMachinegunImage)
{
	item = ClassicLMGItem;
	projectile = ClassicLMGProjectile1;
};

datablock ProjectileData(BlockyLMGProjectile1 : LightMachinegunProjectile1)
{
	directDamage = 12;
};

datablock ItemData(BlockyLMGItem : LightMachinegunItem)
{
	uiName = "Stand-in BlockyLMG";
	image = BlockyLMGImage;
	TT_maxAmmo = 10;
};

datablock ShapeBaseImageData(BlockyLMGImage : LightMachinegunImage)
{
	item = BlockyLMGItem;
	projectile = BlockyLMGProjectile1;
};

function ClassicLMGImage::onFire(%this,%obj,%slot)
{
   TT_knockback(%obj, 0, 0, -1);
   %projectile = %this.projectile;
   %spread = 0.002;
   %shellCount = 1;
   %this.TT_decrementAmmo(%obj);
   return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function ClassicLMGImage::onFire2(%this,%obj,%slot)
{
   TT_knockback(%obj, 0, 0, -1);
   %shellCount = 1;
   %spread = 0.0005;
   %projectile = %this.projectile;
   return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function ClassicLMGImage::onClick(%this,%obj,%slot)
{
   %obj.pushDatablock(LMGArmor.getID());
}

function ClassicLMGImage::onHalt(%this,%obj,%slot)
{
   %obj.popDatablock(LMGArmor.getID());
}

function ClassicLMGImage::onEmptyTransition(%this,%obj,%slot)
{
   %obj.popDatablock(LMGArmor.getID());
}

function ClassicLMGImage::onUnMount(%this,%obj,%slot)
{
   %obj.popDatablock(LMGArmor.getID());
   Parent::onUnMount(%this,%obj,%slot);
}

function ClassicLMGProjectile1::damage(%this,%obj,%col,%fade,%pos,%normal)
{
   if(%col.getType() & $TypeMasks::PlayerObjectType)
      TT_dampenVelocity(%col, 2);
   Parent::damage(%this,%obj,%col,%fade,%pos,%normal);
}

function BlockyLMGImage::onFire(%this,%obj,%slot)
{
   TT_knockback(%obj, 0, 0, -1);
   %projectile = %this.projectile;
   %spread = 0.002;
   %shellCount = 1;
   %this.TT_decrementAmmo(%obj);
   return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function BlockyLMGImage::onFire2(%this,%obj,%slot)
{
   TT_knockback(%obj, 0, 0, -1);
   %shellCount = 1;
   %spread = 0.0005;
   %projectile = %this.projectile;
   return TT_createProjectile(%this, %obj, %slot, %projectile, %shellCount, %spread);
}

function BlockyLMGImage::onClick(%this,%obj,%slot)
{
   %obj.pushDatablock(LMGArmor.getID());
}

function BlockyLMGImage::onHalt(%this,%obj,%slot)
{
   %obj.popDatablock(LMGArmor.getID());
}

function BlockyLMGImage::onEmptyTransition(%this,%obj,%slot)
{
   %obj.popDatablock(LMGArmor.getID());
}

function BlockyLMGImage::onUnMount(%this,%obj,%slot)
{
   %obj.popDatablock(LMGArmor.getID());
   Parent::onUnMount(%this,%obj,%slot);
}

function BlockyLMGProjectile1::damage(%this,%obj,%col,%fade,%pos,%normal)
{
   if(%col.getType() & $TypeMasks::PlayerObjectType)
      TT_dampenVelocity(%col, 2);
   Parent::damage(%this,%obj,%col,%fade,%pos,%normal);
}
