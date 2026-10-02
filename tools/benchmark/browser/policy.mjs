// Browser-runner policy is tested without installing or launching Chromium.
export function clearExternalControl(environment) {
  for (const name of ["SELENIUM_REMOTE_URL", "PWDEBUG"]) {
    for (const key of [
      name,
      `npm_config_${name.toLowerCase()}`,
      `npm_package_config_${name.toLowerCase()}`,
    ]) {
      delete environment[key];
    }
  }
}

export function parseOptions(arguments_) {
  if (arguments_.length !== 6) throw new Error("Expected six runner arguments");
  const [address, mode, widthText, heightText, scaleText, timeoutText] =
    arguments_;
  const url = new URL(address);
  const width = Number(widthText);
  const height = Number(heightText);
  const scale = Number(scaleText);
  const seconds = Number(timeoutText);
  if (
    url.protocol !== "http:" ||
    url.hostname !== "127.0.0.1" ||
    url.username ||
    url.password ||
    !["headless", "headed"].includes(mode) ||
    ![width, height].every(
      (value) => Number.isInteger(value) && value >= 64 && value <= 8192,
    ) ||
    !Number.isFinite(scale) ||
    scale <= 0 ||
    scale > 4 ||
    !Number.isInteger(seconds) ||
    seconds < 1 ||
    seconds > 7200
  ) {
    throw new Error(
      "Expected a local measurement URL, browser mode, dimensions, scale, and timeout",
    );
  }
  return { url, mode, width, height, scale, timeout: seconds * 1000 };
}

export function recordedLaunchArguments(arguments_) {
  return arguments_.map((argument) =>
    argument.startsWith("--user-data-dir=")
      ? "--user-data-dir=[temporary-profile]"
      : argument,
  );
}
