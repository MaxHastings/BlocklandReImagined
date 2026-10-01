// Stand-in for the Weapon_Skins_Pistol port tests (CC0): our own skins on the stand-in
// hosts, under the datablock and method names the port covers.
ForceRequiredAddOn("Weapon_Package_Tier1");

datablock ItemData(ClassicPistolItem : standinSidearmItem)
{
	uiName = "Stand-in ClassicPistol";
	image = ClassicPistolImage;
	TT_maxAmmo = 6;
};

datablock ShapeBaseImageData(ClassicPistolImage : standinSidearmImage)
{
	item = ClassicPistolItem;
	TT_raycastDirectDamage = 12;
};

datablock ItemData(ModernPistolItem : standinSidearmItem)
{
	uiName = "Stand-in ModernPistol";
	image = ModernPistolImage;
	TT_maxAmmo = 8;
};

datablock ShapeBaseImageData(ModernPistolImage : standinSidearmImage)
{
	item = ModernPistolItem;
	TT_raycastDirectDamage = 14;
};

datablock ItemData(SilencedPistolItem : standinSidearmItem)
{
	uiName = "Stand-in SilencedPistol";
	image = SilencedPistolImage;
	TT_maxAmmo = 10;
};

datablock ShapeBaseImageData(SilencedPistolImage : standinSidearmImage)
{
	item = SilencedPistolItem;
	TT_raycastDirectDamage = 16;
};

datablock ItemData(RetroPistolItem : standinSidearmItem)
{
	uiName = "Stand-in RetroPistol";
	image = RetroPistolImage;
	TT_maxAmmo = 5;
};

datablock ShapeBaseImageData(RetroPistolImage : standinSidearmImage)
{
	item = RetroPistolItem;
	TT_raycastDirectDamage = 18;
};

function ClassicPistolImage::onFire(%this,%obj,%slot)
{
   if($Pref::Server::TT::Recoil)
      %obj.spawnExplosion(standinKickProjectile,"1 1 1");

   if(vectorLen(%obj.getVelocity()) > 0.1)
   {
      %this.TT_raycastSpreadAmt = 0.002;
      %this.TT_raycastWeaponRange = 60;
   }
   else
   {
      %this.TT_raycastSpreadAmt = 0.0004;
      %this.TT_raycastWeaponRange = 150;
   }

   %this.TT_decrementAmmo(%obj);
   %this.TT_displayAmmo(%obj);
   %obj.playThread(2, shiftAway);
   return Parent::onFire(%this,%obj,%slot);
}

function ModernPistolImage::onFire(%this,%obj,%slot)
{
   if($Pref::Server::TT::Recoil)
      %obj.spawnExplosion(standinKickProjectile,"1 1 1");

   if(vectorLen(%obj.getVelocity()) > 0.1)
   {
      %this.TT_raycastSpreadAmt = 0.002;
      %this.TT_raycastWeaponRange = 60;
   }
   else
   {
      %this.TT_raycastSpreadAmt = 0.0004;
      %this.TT_raycastWeaponRange = 150;
   }

   %this.TT_decrementAmmo(%obj);
   %this.TT_displayAmmo(%obj);
   %obj.playThread(2, shiftAway);
   return Parent::onFire(%this,%obj,%slot);
}

function SilencedPistolImage::onFire(%this,%obj,%slot)
{
   if($Pref::Server::TT::Recoil)
      %obj.spawnExplosion(standinKickProjectile,"1 1 1");

   if(vectorLen(%obj.getVelocity()) > 0.1)
   {
      %this.TT_raycastSpreadAmt = 0.002;
      %this.TT_raycastWeaponRange = 60;
   }
   else
   {
      %this.TT_raycastSpreadAmt = 0.0004;
      %this.TT_raycastWeaponRange = 150;
   }

   %this.TT_decrementAmmo(%obj);
   %this.TT_displayAmmo(%obj);
   %obj.playThread(2, shiftAway);
   return Parent::onFire(%this,%obj,%slot);
}

function RetroPistolImage::onFire(%this,%obj,%slot)
{
   if($Pref::Server::TT::Recoil)
      %obj.spawnExplosion(standinKickProjectile,"1 1 1");

   if(vectorLen(%obj.getVelocity()) > 0.1)
   {
      %this.TT_raycastSpreadAmt = 0.002;
      %this.TT_raycastWeaponRange = 60;
   }
   else
   {
      %this.TT_raycastSpreadAmt = 0.0004;
      %this.TT_raycastWeaponRange = 150;
   }

   %this.TT_decrementAmmo(%obj);
   %this.TT_displayAmmo(%obj);
   %obj.playThread(2, shiftAway);
   return Parent::onFire(%this,%obj,%slot);
}
