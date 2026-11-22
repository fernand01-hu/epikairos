// Translations for texts built in JavaScript. French is the source language;
// on English pages the server embeds the dictionary (French -> English) as JSON.
(function () {
    let dict = {};
    try {
        const data = document.getElementById("i18nData");
        if (data) dict = JSON.parse(data.textContent);
    } catch (e) { /* keep French */ }

    /** t("Créé le {date}", { date: "1 nov." }) -> translated text with values filled in. */
    window.t = function (text, values) {
        let out = Object.prototype.hasOwnProperty.call(dict, text) ? dict[text] : text;
        if (values) {
            for (const [k, v] of Object.entries(values)) out = out.split(`{${k}}`).join(String(v));
        }
        return out;
    };

    /** Locale for dates and times: "fr-FR" or "en-GB". */
    window.LOCALE = document.documentElement.lang === "en" ? "en-GB" : "fr-FR";
})();
