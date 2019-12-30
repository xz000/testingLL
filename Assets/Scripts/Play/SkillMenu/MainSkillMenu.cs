using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class MainSkillMenu : MonoBehaviour
{

    CanvasGroup cg;

    // Use this for initialization
    void Start()
    {
        cg = GetComponent<CanvasGroup>();
        //OpenMainSkillMenu();
    }

    public void CloseMainSkillMenu()
    {
        if (!cg.interactable)
            return;
        cg.interactable = false;
        cg.alpha = 0;
        //GetComponent<SkillsLink>().selfset();
    }

    public void UnblockRaycasts()
    {
        if (!cg.blocksRaycasts)
            return;
        cg.blocksRaycasts = false;
    }

    public void OpenMainSkillMenu()
    {
        cg.alpha = 1;
        cg.interactable = true;
        cg.blocksRaycasts = true;
    }
}
