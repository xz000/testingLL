using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class MainSkillMenu : MonoBehaviour {

    CanvasGroup cg;

	// Use this for initialization
	void Start ()
    {
        cg = GetComponent<CanvasGroup>();
        //OpenMainSkillMenu();
    }
	
	// Update is called once per frame
	void Update () {
        if (Input.GetKeyDown(KeyCode.Tab))
        {
            if (cg.alpha == 0)
            {
                OpenMainSkillMenu();
            }
            else
                CloseMainSkillMenu();
        }
	}

    public void CloseMainSkillMenu()
    {
        if (cg.alpha == 0)
            return;
        cg.alpha = 0;
        cg.interactable = false;
        cg.blocksRaycasts = false;
        GetComponent<SkillsLink>().selfset();
    }

    public void OpenMainSkillMenu()
    {
        cg.alpha = 1;
        cg.interactable = true;
        cg.blocksRaycasts = true;
    }
}
