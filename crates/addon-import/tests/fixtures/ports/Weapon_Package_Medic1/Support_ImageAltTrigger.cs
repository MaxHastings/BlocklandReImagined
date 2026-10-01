package ImageAltTrigger
{
   function Armor::onTrigger(%db, %player, %triggerSlot, %val)
   {
      for(%i = 0; %i < 4; %i++)
      {
         if(isObject(%image = %player.getMountedImage(%i)) && %image.altTriggerEnabled)
            %image.onAltTrigger(%player, %db, %triggerSlot, %val);
      }
      Parent::onTrigger(%db, %player, %triggerSlot, %val);
   }
};
activatePackage(ImageAltTrigger);
