// Stand-in for the Weapon_Skins_MPistol port tests (CC0): our own skins on the stand-in
// hosts, under the datablock and method names the port covers.
ForceRequiredAddOn("Weapon_Package_Tier2A");

datablock ProjectileData(ModernMPTracerProjectile : MachstilTracerProjectile)
{
	directDamage = 9;
};

datablock ItemData(ModernMPItem : MachstilItem)
{
	uiName = "Stand-in ModernMP";
	image = ModernMPImage;
	TT_maxAmmo = 7;
};

datablock ShapeBaseImageData(ModernMPImage : MachstilImage)
{
	item = ModernMPItem;
	TT_raycastDirectDamage = 13;
};

datablock ProjectileData(ClassicMPTracerProjectile : MachstilTracerProjectile)
{
	directDamage = 12;
};

datablock ItemData(ClassicMPItem : MachstilItem)
{
	uiName = "Stand-in ClassicMP";
	image = ClassicMPImage;
	TT_maxAmmo = 10;
};

datablock ShapeBaseImageData(ClassicMPImage : MachstilImage)
{
	item = ClassicMPItem;
	TT_raycastDirectDamage = 16;
};

function ModernMPImage::onFire(%this,%obj,%slot)
{
   if(vectorLen(%obj.getVelocity()) > 0.1)
   {
      %this.TT_raycastSpreadAmt = 0.0024;
      %this.TT_raycastWeaponRange = 45;
   }
   else
   {
      %this.TT_raycastSpreadAmt = 0.0013;
      %this.TT_raycastWeaponRange = 100;
   }
   Parent::onFire(%this,%obj,%slot);
   %this.TT_decrementAmmo(%obj);
}

function ClassicMPImage::onFire(%this,%obj,%slot)
{
   if(vectorLen(%obj.getVelocity()) > 0.1)
   {
      %this.TT_raycastSpreadAmt = 0.0024;
      %this.TT_raycastWeaponRange = 45;
   }
   else
   {
      %this.TT_raycastSpreadAmt = 0.0013;
      %this.TT_raycastWeaponRange = 100;
   }
   Parent::onFire(%this,%obj,%slot);
   %this.TT_decrementAmmo(%obj);
}
