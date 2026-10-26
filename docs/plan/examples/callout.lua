function CodeBlock(block)
  if not block.classes:includes("callout") then
    return nil
  end

  local content = pandoc.read(block.text, "markdown").blocks
  return pandoc.Div({ pandoc.BlockQuote(content) }, block.attr)
end
