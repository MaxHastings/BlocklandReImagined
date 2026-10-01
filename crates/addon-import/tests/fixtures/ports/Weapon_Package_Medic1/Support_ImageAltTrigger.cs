package ImageAltTrigger
{
   function Armor::onTrigger(%db, %player, %triggerSlot, %val)
   {
	if(isObject(%image = %player.getMountedImage(%i)) && %image.altTriggerEnabled)
	%image.onAltTrigger(%player, %db, %triggerSlot, %val);
}
};
activatePackage(ImageAltTrigger);
