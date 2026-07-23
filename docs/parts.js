(function () {
  var ALWAYS_OPEN = ["Using Amiss"];

  function groups(list) {
    var result = [];
    var current = null;
    Array.prototype.forEach.call(list.children, function (item) {
      if (item.classList.contains("part-title")) {
        current = { title: item, members: [] };
        result.push(current);
      } else if (current) {
        current.members.push(item);
      }
    });
    return result;
  }

  function apply(group, expanded) {
    group.title.setAttribute("aria-expanded", expanded ? "true" : "false");
    group.members.forEach(function (member) {
      member.style.display = expanded ? "" : "none";
    });
  }

  function init() {
    var list = document.querySelector("mdbook-sidebar-scrollbox ol.chapter");
    if (!list || !list.querySelector("li.part-title")) {
      return false;
    }
    groups(list).forEach(function (group) {
      if (ALWAYS_OPEN.indexOf(group.title.textContent.trim()) !== -1) {
        return;
      }
      var holdsPage = group.members.some(function (member) {
        return member.querySelector("a.active") !== null;
      });
      var chevron = document.createElement("span");
      chevron.className = "part-chevron";
      chevron.textContent = "❯";
      group.title.appendChild(chevron);
      group.title.setAttribute("role", "button");
      group.title.setAttribute("tabindex", "0");
      apply(group, holdsPage);
      function toggle() {
        apply(group, group.title.getAttribute("aria-expanded") !== "true");
      }
      group.title.addEventListener("click", toggle);
      group.title.addEventListener("keydown", function (event) {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          toggle();
        }
      });
    });
    return true;
  }

  if (!init()) {
    document.addEventListener("DOMContentLoaded", init);
  }
})();
