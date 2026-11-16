<script lang="ts">
  // The app icon in the window's header (a copy of src-tauri/icons/128x128.png).
  import icon from "$lib/assets/app-icon.png";
  let { size = 22 }: { size?: number } = $props();
</script>

<img src={icon} width={size} height={size} alt="" aria-hidden="true" draggable="false" />

<style>
  img {
    display: block;
    flex: none;
  }
</style>
