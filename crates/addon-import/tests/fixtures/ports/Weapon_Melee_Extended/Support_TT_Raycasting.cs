// Stand-in raycasting support: hits along a ray with the image's
// TT_raycast fields. Written for these tests.
package TT_Raycasting
{
   function WeaponImage::onFire(%this, %obj, %slot)
   {
	if(!%this.TT_raycastEnabled)
	return Parent::onFire(%this, %obj, %slot);
	%start = %obj.getEyePoint();
	%end = vectorAdd(%start, vectorScale(%obj.getEyeVector(), %this.TT_raycastWeaponRange));
	%ray = containerRayCast(%start, %end, %this.TT_raycastWeaponTargets, %obj);
}

   function WeaponImage::TT_onRaycastHit(%this, %obj, %slot, %col, %pos, %normal)
   {
      if(%col.getType() & $TypeMasks::PlayerObjectType)
      {
         %this.TT_onRaycastDamage(%obj, %slot, %col, %pos);
         serverPlay3D(%this.TT_raycastExplosionPlayerSound, %pos);
      }
      else
         serverPlay3D(%this.TT_raycastExplosionBrickSound, %pos);
   }

   function WeaponImage::TT_onRaycastDamage(%this, %obj, %slot, %col, %pos)
   {
	%col.damage(%obj, %pos, mClampF(%this.TT_raycastDirectDamage, -100, 100), %this.TT_raycastDirectDamageType);
}

   function WeaponImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
   {
	return 0;
}
};
activatePackage(TT_Raycasting);
