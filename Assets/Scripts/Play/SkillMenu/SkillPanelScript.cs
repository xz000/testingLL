using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

public class SkillPanelScript : MonoBehaviour {

    Toggle PanelSwitch;

	// Use this for initialization
	void Start () {
        PanelSwitch = GameObject.Find("Toggle " + name.Substring(name.Length - 3, 3)).GetComponent<Toggle>();
        PanelSwitch.onValueChanged.AddListener(OnOffSwitch);
        OnOffSwitch(PanelSwitch.isOn);
	}

    public void OnOffSwitch(bool a)
    {
        if (a)
        {
            gameObject.GetComponent<CanvasGroup>().alpha = 1;
            gameObject.GetComponent<CanvasGroup>().interactable = true;
            gameObject.GetComponent<CanvasGroup>().blocksRaycasts = true;
        }
        else
        {
            gameObject.GetComponent<CanvasGroup>().alpha = 0;
            gameObject.GetComponent<CanvasGroup>().interactable = false;
            gameObject.GetComponent<CanvasGroup>().blocksRaycasts = false;
        }
    }
}
