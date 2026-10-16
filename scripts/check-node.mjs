const [major, minor] = process.versions.node.split(".").map(Number);
if (!((major === 22 && minor >= 12) || major === 24 || major >= 26)) {
  console.error(`OpenCull requires Node 22.12+, 24, or 26+. You are running ${process.version}.
Use the version pinned in .nvmrc: nvm install && nvm use.
Without a version manager: npx --yes --package=node@22.23.3 --package=npm@11.21.1 --call 'npm run check'`);
  process.exit(1);
}
